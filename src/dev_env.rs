//! The developer environment setup.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use eyre::{eyre, WrapErr};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tokio::process::Command;

use crate::cargo_metadata::CargoMetadata;
use crate::dependency_registry::{DependencyRegistry, RustDependencyConfiguration};

#[derive(Default)]
pub struct DevEnvironment {
    build_inputs: HashSet<String>,
    environment_variables: HashMap<String, String>,
    ld_library_path: HashSet<String>,
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

        let registry: DependencyRegistry = serde_json::from_str(include_str!("../registry.json")).wrap_err("Parsing `registry.json`")?;

        let mut found_build_inputs = registry.language_rust.default.build_inputs;
        let mut found_envs = registry.language_rust.default.environment_variables;
        let mut found_ld_inputs = registry.language_rust.default.ld_library_path_inputs;

        for package in metadata.packages {
            let name = package.name;

            if let Some(RustDependencyConfiguration {
                build_inputs: known_build_inputs,
                environment_variables: known_envs,
                ld_library_path_inputs: known_ld_inputs,
            }) = registry.language_rust.dependencies.get(name.as_str())
            {
                let known_build_inputs = known_build_inputs
                    .iter()
                    .map(ToString::to_string)
                    .collect::<HashSet<_>>();
                let known_ld_inputs = known_ld_inputs
                    .iter()
                    .map(ToString::to_string)
                    .collect::<HashSet<_>>();

                tracing::debug!(
                    package_name = %name,
                    buildInputs = %known_build_inputs.iter().join(", "),
                    environment_variables = %known_envs.iter().map(|(k, v)| format!("{k}={v}")).join(", "),
                    "Detected known crate information"
                );
                found_build_inputs = found_build_inputs
                    .union(&known_build_inputs)
                    .cloned()
                    .collect();

                for (known_key, known_value) in known_envs {
                    found_envs.insert(known_key.to_string(), known_value.to_string());
                }

                found_ld_inputs = found_ld_inputs.union(&known_ld_inputs).cloned().collect();
            }

            let metadata_object = match package.metadata {
                Some(metadata_object) => metadata_object,
                None => continue,
            };

            let fsm_object = match metadata_object.fsm {
                Some(fsm_object) => fsm_object,
                None => continue,
            };

            let package_build_inputs = match &fsm_object.build_inputs {
                Some(build_inputs_table) => {
                    let mut package_build_inputs = HashSet::new();
                    for (key, _value) in build_inputs_table.iter() {
                        // TODO(@hoverbear): Add version checking
                        package_build_inputs.insert(key.to_string());
                    }
                    package_build_inputs
                }
                None => Default::default(),
            };

            let package_envs = match &fsm_object.environment_variables {
                Some(envs_table) => {
                    let mut package_envs = HashMap::new();
                    for (key, value) in envs_table.iter() {
                        package_envs.insert(key.to_string(), value.to_string());
                    }
                    package_envs
                }
                None => Default::default(),
            };

            let package_ld_inputs = match &fsm_object.ld_library_path_inputs {
                Some(ld_table) => {
                    let mut package_ld_inputs = HashSet::new();
                    for (key, _value) in ld_table.iter() {
                        // TODO(@hoverbear): Add version checking
                        package_ld_inputs.insert(key.to_string());
                    }
                    package_ld_inputs
                }
                None => Default::default(),
            };

            tracing::debug!(
                package = %name,
                "build-inputs" = %package_build_inputs.iter().join(", "),
                "environment-variables" = %package_envs.iter().map(|(k, v)| format!("{k}={v}")).join(", "),
                "LD_LIBRARY_PATH-inputs" = %package_ld_inputs.iter().join(", "),
                "Detected `package.fsm` in `Crate.toml`"
            );
            found_build_inputs = found_build_inputs
                .union(&package_build_inputs)
                .cloned()
                .collect();

            for (package_env_key, package_env_value) in package_envs {
                found_envs.insert(package_env_key, package_env_value);
            }

            found_ld_inputs = found_ld_inputs.union(&package_ld_inputs).cloned().collect();
        }

        eprintln!(
            "{check} {lang}: {colored_inputs}{maybe_colored_envs}",
            check = "✓".green(),
            lang = "🦀 rust".bold().red(),
            colored_inputs = {
                let mut sorted_build_inputs = found_build_inputs
                    .union(&found_ld_inputs)
                    .collect::<Vec<_>>();
                sorted_build_inputs.sort();
                sorted_build_inputs.iter().map(|v| v.cyan()).join(", ")
            },
            maybe_colored_envs = {
                if !found_envs.is_empty() {
                    let mut sorted_build_inputs =
                        found_envs.iter().map(|(k, _)| k).collect::<Vec<_>>();
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

        self.build_inputs = found_build_inputs;
        self.environment_variables = found_envs;
        self.ld_library_path = found_ld_inputs;

        Ok(())
    }
}
