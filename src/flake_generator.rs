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
    offline: bool,
    disable_telemetry: bool,
) -> color_eyre::Result<TempDir> {
    let project_dir = match project_dir {
        Some(dir) => dir,
        None => std::env::current_dir().wrap_err("Current working directory was invalid")?,
    };
    tracing::debug!("Project directory is '{}'.", project_dir.display());

    let registry = DependencyRegistry::new(offline).await?;
    let mut dev_env = DevEnvironment::new(&registry);

    match dev_env.detect(&project_dir).await {
        Ok(_) => {}
        err @ Err(_) => {
            let wrapped_err = err
                .wrap_err_with(|| {
                    format!(
                        "\
                            `{colored_project_dir}` doesn't contain a project recognized by Riff.\n\
                            Try running `{riff_shell}` in a Rust project directory.\
                    ",
                        colored_project_dir = &project_dir.display().to_string().green(),
                        riff_shell = "riff shell".cyan(),
                    )
                })
                .unwrap_err();
            eprintln!("{wrapped_err}");
            std::process::exit(1);
        }
    };

    // If the user is using an old version of `riff`, we want to let them know.
    // We do it after detecting the dependencies because we'd prefer the user's first
    // output from the program not to be a scary error, especially when it's neither scary or an
    // error.
    let latest_riff_version = registry.latest_riff_version().await;
    // We don't want to error anywhere here
    if latest_riff_version.as_ref().and_then(|v| semver::Version::parse(&v).ok())
        .and_then(|registry_version| semver::Version::parse(env!("CARGO_PKG_VERSION")).ok()
            .map(|current_version| registry_version > current_version)
        ).unwrap_or(false)
    {
        eprintln!(
            "📦 A new version of `{riff}` ({latest_riff_version_colored}) is available! {riff_download_url}",
            riff = "riff".cyan(),
            latest_riff_version_colored = latest_riff_version.as_ref().cloned().unwrap_or_else(|| "unknown".to_string()).yellow(),
            riff_download_url = "https://riff.determinate.systems/download".blue().underline(),
        );
    }

    if !(disable_telemetry || offline) {
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

    if offline {
        nix_lock_command.arg("--offline");
    }

    tracing::trace!(command = ?nix_lock_command.as_std(), "Running");
    let spinner = SimpleSpinner::new_with_message(Some("Running `nix flake lock`"))
        .context("Failed to construct progress spinner")?;

    let nix_lock_exit = match nix_lock_command.output().await {
        Ok(nix_lock_exit) => nix_lock_exit,
        err @ Err(_) => {
            let wrapped_err = err
                .wrap_err_with(|| {
                    format!(
                        "\
                    Could not execute `{nix_lock}`. Is `{nix}` installed?\n\n\
                    Get instructions for installing Nix: {nix_install_url}\n\
                    Underlying error\
                    ",
                        nix_lock = "nix flake lock".cyan(),
                        nix = "nix".cyan(),
                        nix_install_url = "https://nixos.org/download.html".blue().underline(),
                    )
                })
                .unwrap_err();
            eprintln!("{wrapped_err:#}");
            std::process::exit(1);
        }
    };

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
name = "riff-test"
version = "0.1.0"
edition = "2021"

[lib]
name = "riff_test"
path = "lib.rs"

[dependencies]
        "#,
        )
        .await?;

        let flake_dir =
            generate_flake_from_project_dir(Some(temp_dir.path().to_owned()), true, true).await?;
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
