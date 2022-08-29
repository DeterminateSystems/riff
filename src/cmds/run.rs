//! The `run` subcommand.

use std::path::PathBuf;

use clap::Args;
use eyre::WrapErr;
use owo_colors::OwoColorize;

use crate::flake_generator;

/// Run a command with your project's dependencies
///
/// For example, run `cargo build` inside riff:
///
///     $ riff run cargo build
///
/// Run cargo check and cargo build at the same time:
///
///     $ riff run -- sh -c 'cargo check && cargo build'
#[derive(Debug, Args)]
pub struct Run {
    /// The root directory of the project
    #[clap(long, value_parser)]
    project_dir: Option<PathBuf>,
    /// The command to run with your project's dependencies
    #[clap(required = true)]
    pub(crate) command: Vec<String>,
    #[clap(from_global)]
    disable_telemetry: bool,
    #[clap(from_global)]
    offline: bool,
    // TODO(@cole-h): support additional nix develop args?
}

impl Run {
    pub async fn cmd(&self) -> color_eyre::Result<Option<i32>> {
        let flake_dir = flake_generator::generate_flake_from_project_dir(
            self.project_dir.clone(),
            self.offline,
            self.disable_telemetry,
        )
        .await?;

        let dev_env = crate::nix_dev_env::get_nix_dev_env(flake_dir.path()).await?;

        let command_name = &self.command[0];

        let mut command = crate::nix_dev_env::run_in_dev_env(&dev_env, command_name).await?;

        command.args(&self.command[1..]);

        Ok(command
            .spawn()
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::NotFound {
                    eprintln!(
                        "The command you attempted to run was not found.
Try running it in a shell; for example:
\t{fsm_run_example}\n",
                        fsm_run_example =
                            format!("fsm run -- sh -c '{}'", self.command.join(" ")).cyan(),
                    );
                };
                err
            })
            .wrap_err(format!("Cannot run the command `{}`", command_name))?
            .wait_with_output()
            .await?
            .status
            .code())
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
name = "riff-test"
version = "0.1.0"
edition = "2021"

[lib]
name = "riff_test"
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
            offline: true,
            disable_telemetry: true,
        };

        let run_cmd = tokio_test::task::spawn(run.cmd());
        let run_cmd = tokio_test::block_on(run_cmd);
        assert_eq!(run_cmd.unwrap(), Some(6));
    }
}
