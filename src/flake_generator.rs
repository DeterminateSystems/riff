use std::path::PathBuf;

use eyre::{eyre, WrapErr};
use owo_colors::OwoColorize;
use tempfile::TempDir;
use tokio::process::Command;

use crate::dependency_registry::DependencyRegistry;
use crate::dev_env::DevEnvironment;
use crate::spinner::SimpleSpinner;
use crate::telemetry::Telemetry;

/// Generates a `flake.nix` by inspecting the specified `project_dir` for supported project types.
#[tracing::instrument(skip(disable_telemetry))]
pub async fn generate_flake_from_project_dir(
    project_dir: Option<PathBuf>,
    disable_telemetry: bool,
) -> color_eyre::Result<TempDir> {
    let project_dir = match project_dir {
        Some(dir) => dir,
        None => std::env::current_dir().wrap_err("Current working directory was invalid")?,
    };
    tracing::debug!("Project directory is '{}'.", project_dir.display());

    let registry = DependencyRegistry::new(disable_telemetry).await?;
    let mut dev_env = DevEnvironment::new(registry);

    match dev_env.detect(&project_dir).await {
        Ok(_) => {}
        err @ Err(_) => {
            let wrapped_err = err
                .wrap_err_with(|| {
                    format!(
                        "\
                            `{colored_project_dir}` doesn't contain a project recognized by FSM.\n\
                            Try running `{fsm_shell}` in a Rust project directory.\
                    ",
                        colored_project_dir = &project_dir.display().to_string().green(),
                        fsm_shell = "fsm shell".cyan(),
                    )
                })
                .unwrap_err();
            eprintln!("{wrapped_err}");
            std::process::exit(1);
        }
    };

    if !disable_telemetry {
        match Telemetry::new()
            .await
            .with_detected_languages(&dev_env.detected_languages)
            .send()
            .await
        {
            Ok(_) => (),
            Err(err) => tracing::debug!(%err, "Could not send telemetry"),
        };
    }

    let flake_nix = dev_env.to_flake();
    tracing::trace!("Generated 'flake.nix':\n{}", flake_nix);

    let flake_dir = TempDir::new()?;
    let flake_nix_path = flake_dir.path().join("flake.nix");

    tokio::fs::write(&flake_nix_path, &flake_nix)
        .await
        .wrap_err("Unable to write flake.nix")?;

    let mut nix_lock_command = Command::new("nix");
    nix_lock_command
        .arg("flake")
        .arg("lock")
        .args(&["--extra-experimental-features", "flakes nix-command"])
        .arg("-L")
        .arg(format!("path://{}", flake_dir.path().to_str().unwrap()));

    tracing::trace!(command = ?nix_lock_command, "Running");
    let spinner = SimpleSpinner::new_with_message(Some("Running `nix flake lock`"))
        .context("Failed to construct progress spinner")?;

    let nix_lock_exit = nix_lock_command
        .output()
        .await
        .wrap_err("Could not execute `nix flake lock`")?;

    spinner.finish_and_clear();

    if !nix_lock_exit.status.success() {
        return Err(eyre!(
            "`nix flake lock` exited with code {}:\n{}",
            nix_lock_exit
                .status
                .code()
                .map(|x| x.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            std::str::from_utf8(&nix_lock_exit.stderr)?,
        ));
    }

    Ok(flake_dir)
}

#[cfg(test)]
mod tests {
    use super::generate_flake_from_project_dir;
    use tempfile::TempDir;
    use tokio::fs::{read_to_string, write};

    // We can't run this test by default because it calls Nix. Calling Nix inside Nix doesn't appear
    // to work very well (at least, for this use case).
    #[tokio::test]
    #[ignore]
    async fn generate_flake_success() -> eyre::Result<()> {
        let cache_dir = TempDir::new()?;
        std::env::set_var("XDG_CACHE_HOME", cache_dir.path());
        let temp_dir = TempDir::new()?;
        write(temp_dir.path().join("lib.rs"), "fn main () {}").await?;
        write(
            temp_dir.path().join("Cargo.toml"),
            r#"
[package]
name = "fsm-test"
version = "0.1.0"
edition = "2021"

[lib]
name = "fsm_test"
path = "lib.rs"

[dependencies]
        "#,
        )
        .await?;

        let flake_dir =
            generate_flake_from_project_dir(Some(temp_dir.path().to_owned()), true).await?;
        let flake = read_to_string(flake_dir.path().join("flake.nix")).await?;

        assert!(
            flake.contains("buildInputs = [")
                && flake.contains("cargo")
                && flake.contains("rustfmt")
                && flake.contains("rustc")
        );
        Ok(())
    }

    // NOTE: we can't test the failure case since it will `std::process::exit`
}
