//! The `shell` subcommand.
use std::path::PathBuf;

use clap::Args;
use eyre::WrapErr;

use crate::flake_generator;

/// Start a development shell
#[derive(Debug, Args, Clone)]
pub struct Shell {
    /// The root directory of the project
    #[clap(long, value_parser)]
    project_dir: Option<PathBuf>,
    #[clap(from_global)]
    disable_telemetry: bool,
    #[clap(from_global)]
    offline: bool,
}

impl Shell {
    pub async fn cmd(self) -> color_eyre::Result<Option<i32>> {
        let project_dir = crate::cmds::get_project_dir(&self.project_dir)?;

        let flake_dir = flake_generator::generate_flake_from_project_dir(
            &project_dir,
            self.offline,
            self.disable_telemetry,
        )
        .await?;

        let dev_env = crate::nix_dev_env::get_nix_dev_env(flake_dir.path()).await?;

        let shell = crate::nix_dev_env::get_shell().await?;

        Ok(crate::nix_dev_env::run_in_dev_env(&dev_env, &shell)
            .await?
            .spawn()
            .wrap_err(format!("Cannot run the shell `{}`", shell))?
            .wait_with_output()
            .await?
            .status
            .code())
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
name = "riff-test"
version = "0.1.0"
edition = "2021"

[lib]
name = "riff_test"
path = "lib.rs"

[package.metadata.riff.environment-variables]
shellHook = "exit 6"

[dependencies]
        "#,
        )
        .await?;

        let shell = Shell {
            project_dir: Some(temp_dir.path().to_owned()),
            offline: true,
            disable_telemetry: true,
        };

        let shell_cmd = shell.cmd().await?;
        assert_eq!(shell_cmd, Some(6));
        Ok(())
    }
}
