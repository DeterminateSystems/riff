//! The `shell` subcommand.

use std::path::PathBuf;
use std::process::Stdio;

use clap::Args;
use eyre::{eyre, WrapErr};
use tempfile::TempDir;
use tokio::process::Command;

use crate::dev_env::DevEnvironment;

/// Start a development shell
#[derive(Debug, Args)]
pub struct Shell {
    /// The root directory of the project
    #[clap(long, value_parser)]
    project_dir: Option<PathBuf>,
}

impl Shell {
    // TODO(@cole-h): should this be a trait method? we'll see once we add another subcommand
    pub async fn cmd(self) -> color_eyre::Result<()> {
        let project_dir = match self.project_dir {
            Some(dir) => dir,
            None => std::env::current_dir().wrap_err("Current working directory was invalid")?,
        };
        tracing::debug!("Project directory is '{}'.", project_dir.display());

        let mut dev_env = DevEnvironment::default();
        dev_env.detect(&project_dir).await?;

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
        let nix_lock_exit = nix_lock_command
            .output()
            .await
            .wrap_err("Could not execute `nix flake lock`")?;

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

        if let Some(code) = nix_develop_exit.status.code() {
            // If the user returns, say, an EOF, we return the same code up
            std::process::exit(code);
        }

        Ok(())
    }
}
