//! The `shell` subcommand.

use std::path::PathBuf;
use std::process::Stdio;

use clap::Args;
use eyre::WrapErr;
use tokio::process::Command;

use crate::flake_generator;

/// Start a development shell
#[derive(Debug, Args, Clone)]
pub struct Shell {
    /// The root directory of the project
    #[clap(long, value_parser)]
    project_dir: Option<PathBuf>,
    #[clap(from_global)]
    disable_telemetry: bool,
}

impl Shell {
    pub async fn cmd(self) -> color_eyre::Result<Option<i32>> {
        let flake_dir = flake_generator::generate_flake_from_project_dir(
            self.project_dir,
            self.disable_telemetry,
        )
        .await?;

        let mut nix_develop_command = Command::new("nix");
        nix_develop_command
            .arg("develop")
            .args(&["--extra-experimental-features", "flakes nix-command"])
            .arg("-L")
            .arg(format!("path://{}", flake_dir.path().to_str().unwrap()))
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        tracing::trace!(command = ?nix_develop_command, "Running");
        let nix_develop_exit = nix_develop_command
            .spawn()
            .wrap_err("Failed to spawn `nix develop`")?
            .wait_with_output()
            .await
            .wrap_err("Could not execute `nix develop`")?;

        // At this point we have handed off to the user shell. The next lines run after the user CTRL+D's out.

        Ok(nix_develop_exit.status.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs::write;

    // We can't run this test by default because it calls Nix. Calling Nix inside Nix doesn't appear
    // to work very well (at least, for this use case). We also don't want to run this in CI because
    // the shell is not interactive, leading `nix develop` to exit without evaluating the
    // `shellHook` (and thus thwarting our attempt to check if the shell actually worked by
    // inspecting the exit code).
    #[tokio::test]
    #[ignore]
    async fn shell_succeeds() -> eyre::Result<()> {
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

[package.metadata.fsm.environment-variables]
shellHook = "exit 6"

[dependencies]
        "#,
        )
        .await?;

        let shell = Shell {
            project_dir: Some(temp_dir.path().to_owned()),
            disable_telemetry: true,
        };

        let shell_cmd = shell.cmd().await?;
        assert_eq!(shell_cmd, Some(6));
        Ok(())
    }
}
