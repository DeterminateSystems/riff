//! The `run` subcommand.

use std::{path::PathBuf, process::Stdio};

use clap::Args;
use eyre::WrapErr;
use owo_colors::OwoColorize;
use tokio::process::Command;

use crate::flake_generator;

/// print shell code that can be sourced by bash to reproduce the riff environment
///
/// For example, run `cargo build` inside riff:
///
///     $ eval $(riff print-dev-env)
#[derive(Debug, Args)]
pub struct PrintDevEnv {
    /// The root directory of the project
    #[clap(long, value_parser)]
    project_dir: Option<PathBuf>,
    #[clap(from_global)]
    disable_telemetry: bool,
    #[clap(from_global)]
    offline: bool,
}

impl PrintDevEnv {
    pub async fn cmd(&self) -> color_eyre::Result<Option<i32>> {
        let flake_dir = flake_generator::generate_flake_from_project_dir(
            self.project_dir.clone(),
            self.offline,
            self.disable_telemetry,
        )
        .await?;

        let mut nix_print_dev_env_command = Command::new("nix");
        nix_print_dev_env_command
            .arg("print-dev-env")
            .args(&["--extra-experimental-features", "flakes nix-command"])
            .arg("-L")
            .arg(format!("path://{}", flake_dir.path().to_str().unwrap()))
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        // TODO(@hoverbear): Try to enable this somehow. Right now since we don't keep the lock
        // in a consistent place, we can't reliably pick up a lock generated in online mode.
        //
        // If we stored the generated flake/lock in a consistent place this could be enabled.
        //
        // if self.offline {
        //     nix_develop_command.arg("--offline");
        // }

        tracing::trace!(command = ?nix_print_dev_env_command.as_std(), "Running");
        let nix_print_dev_env_exit = match nix_print_dev_env_command
            .spawn()
            .wrap_err("Failed to spawn `nix print-dev-env`")?
            .wait_with_output()
            .await
        {
            Ok(nix_print_dev_env_exit) => nix_print_dev_env_exit,
            err @ Err(_) => {
                let wrapped_err = err
                    .wrap_err_with(|| {
                        format!(
                            "\
                        Could not execute `{nix_print_dev_env}`. Is `{nix}` installed?\n\n\
                        Get instructions for installing Nix: {nix_install_url}\n\
                        Underlying error\
                        ",
                            nix_print_dev_env = "nix print-dev-env".cyan(),
                            nix = "nix".cyan(),
                            nix_install_url = "https://nixos.org/download.html".blue().underline(),
                        )
                    })
                    .unwrap_err();
                eprintln!("{wrapped_err:#}");
                std::process::exit(1);
            }
        };

        Ok(nix_print_dev_env_exit.status.code())
    }
}
