//! The developer environment setup.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use eyre::{eyre, WrapErr};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tokio::process::Command;

use crate::cargo_metadata::CargoMetadata;
use crate::registry::{KnownCrateRegistryValue, KNOWN_CRATE_REGISTRY};

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

        let mut found_build_inputs = HashSet::new();
        let mut found_envs = HashMap::new();
        let mut found_ld_inputs = HashSet::new();

        found_build_inputs.insert("rustc".to_string());
        found_build_inputs.insert("cargo".to_string());
        found_build_inputs.insert("rustfmt".to_string());

        for package in metadata.packages {
            let name = package.name;

            if let Some(KnownCrateRegistryValue {
                build_inputs: known_build_inputs,
                environment_variables: known_envs,
                ld_library_path_inputs: known_ld_inputs,
            }) = KNOWN_CRATE_REGISTRY.get(name.as_str())
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

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::DevEnvironment;

    #[test]
    fn dev_env_to_flake() {
        let dev_env = DevEnvironment {
            build_inputs: ["cargo", "hello"]
                .into_iter()
                .map(ToString::to_string)
                .collect(),
            environment_variables: [("HELLO", "WORLD"), ("GOODBYE", "WORLD")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ld_library_path: ["nix", "libGL"]
                .into_iter()
                .map(ToString::to_string)
                .collect(),
        };

        let flake = dev_env.to_flake();
        eprintln!("{}", &flake);
        assert!(
            flake.contains("buildInputs = [") && flake.contains("cargo") && flake.contains("hello")
        );
        assert!(flake.contains(r#""GOODBYE" = "WORLD""#));
        assert!(flake.contains(r#""HELLO" = "WORLD""#));
        assert!(
            flake.contains(r#""LD_LIBRARY_PATH" = "#)
                && flake.contains("${lib.getLib nix}/lib")
                && flake.contains("${lib.getLib libGL}/lib")
        );
    }

    #[test]
    fn dev_env_detect_supported_project() {
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

[package.metadata.fsm.build-inputs]
hello = "*"

[package.metadata.fsm.environment-variables]
HI = "BYE"

[package.metadata.fsm.LD_LIBRARY_PATH-inputs]
libGL = "*"

[dependencies]
        "#,
        )
        .unwrap();

        let mut dev_env = DevEnvironment::default();
        let detect = tokio_test::block_on(dev_env.detect(temp_dir.path()));
        assert!(detect.is_ok());

        assert!(dev_env.build_inputs.get("hello").is_some());
        assert_eq!(
            dev_env.environment_variables.get("HI"),
            Some(&String::from("BYE"))
        );
        assert!(dev_env.ld_library_path.get("libGL").is_some());
    }

    #[test]
    fn dev_env_detect_unsupported_project() {
        let temp_dir = TempDir::new().unwrap();
        let mut dev_env = DevEnvironment::default();
        let detect = tokio_test::block_on(dev_env.detect(temp_dir.path()));
        assert!(detect.is_err());
    }
}
