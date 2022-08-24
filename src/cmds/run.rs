//! The `run` subcommand.

use std::{path::PathBuf, process::Stdio};

use clap::Args;
use eyre::WrapErr;
use tokio::process::Command;

use crate::flake_generator;

/// Run a command with your project's dependencies
///
/// For example, run `cargo build` inside fsm:
///
///     $ fsm run cargo build
///
/// Run cargo check and cargo build at the same time:
///
///     $ fsm run -- sh -c 'cargo check && cargo build'
#[derive(Debug, Args)]
pub struct Run {
    /// The root directory of the project
    #[clap(long, value_parser)]
    project_dir: Option<PathBuf>,
    /// The command to run with your project's dependencies
    #[clap(required = true)]
    pub(crate) command: Vec<String>,
    // TODO(@cole-h): support additional nix develop args?
}

impl Run {
    pub async fn cmd(&self) -> color_eyre::Result<Option<i32>> {
        let flake_dir =
            flake_generator::generate_flake_from_project_dir(self.project_dir.clone()).await?;

        let mut nix_develop_command = Command::new("nix");
        nix_develop_command
            .arg("develop")
            .args(&["--extra-experimental-features", "flakes nix-command"])
            .arg("-L")
            .arg(format!("path://{}", flake_dir.path().to_str().unwrap()))
            .arg("-c")
            .args(self.command.clone())
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        tracing::trace!(command = ?nix_develop_command, "Running");
        let nix_develop_exit = nix_develop_command
            .output()
            .await
            .wrap_err("Could not execute `nix develop`")?;

        Ok(nix_develop_exit.status.code())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::Run;

    // We can't run this test by default because it calls Nix. Calling Nix inside Nix doesn't appear
    // to work very well (at least, for this use case).
    #[test]
    #[ignore]
    fn run_succeeds() {
        let cache_dir = TempDir::new().unwrap();
        std::env::set_var("XDG_CACHE_HOME", cache_dir.path());
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("lib.rs"), "fn main () {}").unwrap();
        std::fs::write(
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
        .unwrap();

        let run = Run {
            project_dir: Some(temp_dir.path().to_owned()),
            command: ["sh", "-c", "exit 6"]
                .into_iter()
                .map(String::from)
                .collect(),
        };

        let run_cmd = tokio_test::task::spawn(run.cmd());
        let run_cmd = tokio_test::block_on(run_cmd);
        assert_eq!(run_cmd.unwrap(), Some(6));
    }
}
