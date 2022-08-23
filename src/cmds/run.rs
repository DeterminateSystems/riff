//! The `run` subcommand.

use std::{ffi::OsString, path::PathBuf, process::Stdio};

use clap::Args;
use eyre::WrapErr;
use tokio::process::Command;

/// Run a command in a development shell
#[derive(Debug, Args)]
pub struct Run {
    /// The root directory of the project
    #[clap(long, value_parser)]
    project_dir: Option<PathBuf>,
    /// The command to run in the project's development shell
    #[clap(required = true)]
    command: Vec<OsString>,
    // TODO(@cole-h): support additional nix develop args?
}

impl Run {
    pub async fn cmd(self) -> color_eyre::Result<Option<i32>> {
        let flake_dir = super::generate_flake_from_project_dir(self.project_dir).await?;

        let mut nix_develop_command = Command::new("nix");
        nix_develop_command
            .arg("develop")
            .args(&["--extra-experimental-features", "flakes nix-command"])
            .arg("-L")
            .arg(format!("path://{}", flake_dir.path().to_str().unwrap()))
            .arg("-c")
            .args(self.command)
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

        Ok(nix_develop_exit.status.code())
    }
}
