use std::path::PathBuf;

use clap::Args;
use eyre::eyre;

use crate::flake_generator;

/// Generate a 'flake.nix' file
///
/// For example, to create a 'flake.nix' for the project in the
/// current directory:
///
///     $ riff generate
#[derive(Debug, Args)]
pub struct Generate {
    /// The root directory of the project
    #[clap(long, value_parser)]
    project_dir: Option<PathBuf>,
    #[clap(from_global)]
    disable_telemetry: bool,
    #[clap(from_global)]
    offline: bool,
    /// Write the generated `flake.nix` to stdout.
    #[clap(long)]
    stdout: bool,
}

impl Generate {
    pub async fn cmd(&self) -> color_eyre::Result<()> {
        let project_dir = crate::cmds::get_project_dir(&self.project_dir)?;

        let flake_dir = flake_generator::generate_flake_from_project_dir(
            &project_dir,
            self.offline,
            self.disable_telemetry,
        )
        .await?;

        if self.stdout {
            let s = tokio::fs::read_to_string(flake_dir.path().join("flake.nix")).await?;
            println!("{}", s);
        } else {
            for filename in ["flake.nix", "flake.lock"] {
                let src_path = flake_dir.path().join(filename);
                let dst_path = project_dir.join(filename);

                if dst_path.exists() {
                    return Err(eyre!(
                        "File `{}` already exists, refusing to overwrite.",
                        dst_path.display()
                    ));
                }

                tokio::fs::copy(&src_path, &dst_path).await?;
            }
        }

        Ok(())
    }
}
