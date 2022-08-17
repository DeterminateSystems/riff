//! The developer environment setup.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use eyre::{eyre, WrapErr};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tokio::process::Command;

use crate::cargo_metadata::CargoMetadata;
use crate::dependency_registry::DependencyRegistry;

#[derive(Default)]
pub struct DevEnvironment {
    pub(crate) build_inputs: HashSet<String>,
    pub(crate) environment_variables: HashMap<String, String>,
    pub(crate) ld_library_path: HashSet<String>,
}

// TODO(@cole-h): should this become a trait that the various languages we may support have to implement?
impl DevEnvironment {
    pub fn to_flake(&self) -> String {
        // TODO: use rnix for generating Nix?
        format!(
            include_str!("flake-template.inc"),
            build_inputs = self.build_inputs.iter().join(" "),
            environment_variables = self
                .environment_variables
                .iter()
                .map(|(name, value)| format!("\"{}\" = \"{}\";", name, value))
                .join("\n"),
            ld_library_path = if !self.ld_library_path.is_empty() {
                format!(
                    "\"LD_LIBRARY_PATH\" = \"{}\";",
                    self.ld_library_path
                        .iter()
                        .map(|v| format!("${{lib.getLib {v}}}/lib"))
                        .join(":")
                )
            } else {
                "".to_string()
            }
        )
    }

    pub async fn detect(&mut self, project_dir: &Path) -> color_eyre::Result<()> {
        if project_dir.join("Cargo.toml").exists() {
            self.add_deps_from_cargo(project_dir).await?;
            Ok(())
        } else {
            Err(eyre!(
                "'{}' does not contain a project recognized by FSM.",
                project_dir.display()
            ))
        }
    }

    #[tracing::instrument(skip_all, fields(project_dir = %project_dir.display()))]
    async fn add_deps_from_cargo(&mut self, project_dir: &Path) -> color_eyre::Result<()> {
        tracing::debug!("Adding Cargo dependencies...");

        let mut cargo_metadata_command = Command::new("cargo");
        cargo_metadata_command.args(&["metadata", "--format-version", "1"]);
        cargo_metadata_command.arg("--manifest-path");
        cargo_metadata_command.arg(project_dir.join("Cargo.toml"));

        tracing::trace!(command = ?cargo_metadata_command, "Running");
        let cargo_metadata_output = cargo_metadata_command
            .output()
            .await
            .wrap_err("Could not execute `cargo metadata`")?;

        if !cargo_metadata_output.status.success() {
            return Err(eyre!(
                "`cargo metadata` exited with code {}:\n{}",
                cargo_metadata_output
                    .status
                    .code()
                    .map(|x| x.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                std::str::from_utf8(&cargo_metadata_output.stderr)?,
            ));
        }

        let cargo_metdata_output = std::str::from_utf8(&cargo_metadata_output.stdout)
            .wrap_err("Output produced by `cargo metadata` was not valid UTF8")?;
        let metadata: CargoMetadata = serde_json::from_str(cargo_metdata_output).wrap_err(
            "Unable to parse output produced by `cargo metadata` into our desired structure",
        )?;

        let registry: DependencyRegistry = serde_json::from_str(include_str!("../registry.json"))
            .wrap_err("Parsing `registry.json`")?;
        if registry.version != 1 {
            return Err(eyre!("Wrong registry version"));
        }

        registry.language_rust.default.try_apply(self)?;

        for package in metadata.packages {
            let name = package.name;

            if let Some(dep_config) = registry.language_rust.dependencies.get(name.as_str()) {
                tracing::debug!(
                    package_name = %name,
                    buildInputs = %dep_config.build_inputs.iter().join(", "),
                    environment_variables = %dep_config.environment_variables.iter().map(|(k, v)| format!("{k}={v}")).join(", "),
                    LD_LIBRARY_PATH = %dep_config.ld_library_path_inputs.iter().join(", "),
                    "Detected known crate information"
                );
                dep_config.clone().try_apply(self)?;
            }

            let metadata_object = match package.metadata {
                Some(metadata_object) => metadata_object,
                None => continue,
            };

            let dep_config = match metadata_object.fsm {
                Some(fsm_object) => fsm_object,
                None => continue,
            };

            tracing::debug!(
                package = %name,
                "build-inputs" = %dep_config.build_inputs.iter().join(", "),
                "environment-variables" = %dep_config.environment_variables.iter().map(|(k, v)| format!("{k}={v}")).join(", "),
                "LD_LIBRARY_PATH-inputs" = %dep_config.ld_library_path_inputs.iter().join(", "),
                "Detected `package.fsm` in `Crate.toml`"
            );
            dep_config.try_apply(self)?;
        }

        eprintln!(
            "{check} {lang}: {colored_inputs}{maybe_colored_envs}",
            check = "✓".green(),
            lang = "🦀 rust".bold().red(),
            colored_inputs = {
                let mut sorted_build_inputs = self
                    .build_inputs
                    .union(&self.ld_library_path)
                    .collect::<Vec<_>>();
                sorted_build_inputs.sort();
                sorted_build_inputs.iter().map(|v| v.cyan()).join(", ")
            },
            maybe_colored_envs = {
                if !self.environment_variables.is_empty() {
                    let mut sorted_build_inputs = self
                        .environment_variables
                        .iter()
                        .map(|(k, _)| k)
                        .collect::<Vec<_>>();
                    sorted_build_inputs.sort();
                    format!(
                        " ({})",
                        sorted_build_inputs.iter().map(|v| v.green()).join(", ")
                    )
                } else {
                    "".to_string()
                }
            }
        );

        Ok(())
    }
}
