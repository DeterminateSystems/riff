//! The developer environment setup.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};

use crate::dependency_registry::DependencyRegistry;
use crate::metadata::{javascript::PackageJson, rust::CargoMetadata};
use crate::spinner::SimpleSpinner;
use eyre::{eyre, WrapErr};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
pub enum DetectedLanguage {
    Rust,
    Javascript,
}

#[derive(Debug, Clone)]
pub struct DevEnvironment<'a> {
    pub(crate) registry: &'a DependencyRegistry,
    pub(crate) build_inputs: HashSet<String>,
    pub(crate) environment_variables: HashMap<String, String>,
    pub(crate) runtime_inputs: HashSet<String>,
    pub(crate) detected_languages: HashSet<DetectedLanguage>,
}

// TODO(@cole-h): should this become a trait that the various languages we may support have to implement?
impl<'a> DevEnvironment<'a> {
    pub fn new(registry: &'a DependencyRegistry) -> Self {
        Self {
            registry,
            build_inputs: Default::default(),
            environment_variables: Default::default(),
            runtime_inputs: Default::default(),
            detected_languages: Default::default(),
        }
    }
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
            ld_library_path = if !self.runtime_inputs.is_empty() {
                format!(
                    "\"LD_LIBRARY_PATH\" = \"{}\";",
                    self.runtime_inputs
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
            self.detected_languages.insert(DetectedLanguage::Rust);
            self.add_deps_from_cargo_toml(project_dir).await?;
            Ok(())
        } else if project_dir.join("package.json").exists() {
            self.detected_languages.insert(DetectedLanguage::Javascript);
            self.add_deps_from_package_json(project_dir).await?;
            Ok(())
        } else {
            Err(eyre!(
                "'{}' does not contain a project recognized by Riff.",
                project_dir.display()
            ))
        }
    }

    #[tracing::instrument(skip_all, fields(project_dir = %project_dir.display()))]
    async fn add_deps_from_cargo_toml(&mut self, project_dir: &Path) -> color_eyre::Result<()> {
        tracing::debug!("Adding Cargo dependencies...");

        let mut cargo_metadata_command = Command::new("cargo");
        cargo_metadata_command.args(&["metadata", "--format-version", "1"]);
        cargo_metadata_command.arg("--manifest-path");
        cargo_metadata_command.arg(project_dir.join("Cargo.toml"));

        // Infer offline-ness from our stored registry
        if self.registry.offline() {
            cargo_metadata_command.arg("--offline");
        }

        tracing::trace!(command = ?cargo_metadata_command.as_std(), "Running");
        let spinner = SimpleSpinner::new_with_message(Some(&format!(
            "Running `{cargo_metadata}`",
            cargo_metadata = "cargo metadata".cyan()
        )))
        .context("Failed to construct progress spinner")?;

        let cargo_metadata_output = match cargo_metadata_command.output().await {
            Ok(output) => output,
            Err(err) => {
                let err_msg = format!(
                    "\
                    Could not execute `{cargo_metadata}`. Is `{cargo}` installed?\n\n\
                    Get instructions for installing Cargo: {rust_install_url}\
                    ",
                    cargo_metadata = "cargo metadata".cyan(),
                    cargo = "cargo".cyan(),
                    rust_install_url = "https://www.rust-lang.org/tools/install".blue().underline()
                );
                eprintln!("{err_msg}\n\nUnderlying error:\n{err}", err = err.red());
                std::process::exit(1);
            }
        };

        spinner.finish_and_clear();

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

        let cargo_metadata_output = std::str::from_utf8(&cargo_metadata_output.stdout)
            .wrap_err("Output produced by `cargo metadata` was not valid UTF8")?;
        let metadata: CargoMetadata = serde_json::from_str(cargo_metadata_output).wrap_err(
            "Unable to parse output produced by `cargo metadata` into our desired structure",
        )?;

        tracing::debug!(fresh = %self.registry.fresh(), "Cache freshness");
        let language_registry = self.registry.language().await.clone();
        language_registry.rust.default.apply(self);

        for package in metadata.packages {
            let name = package.name;

            if let Some(dep_config) = language_registry.rust.dependencies.get(name.as_str()) {
                tracing::debug!(
                    package_name = %name,
                    "build-inputs" = %dep_config.build_inputs().iter().join(", "),
                    "environment-variables" = %dep_config.environment_variables().iter().map(|(k, v)| format!("{k}={v}")).join(", "),
                    "runtime-inputs" = %dep_config.runtime_inputs().iter().join(", "),
                    "Detected known crate information"
                );
                dep_config.clone().apply(self);
            }

            let metadata_object = match package.metadata {
                Some(metadata_object) => metadata_object,
                None => continue,
            };

            let dep_config = match metadata_object.riff {
                Some(riff_object) => riff_object,
                None => continue,
            };

            tracing::debug!(
                package = %name,
                "build-inputs" = %dep_config.build_inputs().iter().join(", "),
                "environment-variables" = %dep_config.environment_variables().iter().map(|(k, v)| format!("{k}={v}")).join(", "),
                "runtime-inputs" = %dep_config.runtime_inputs().iter().join(", "),
                "Detected `package.metadata.riff` in `Crate.toml`"
            );
            dep_config.apply(self);
        }

        eprintln!(
            "{check} {lang}: {colored_inputs}{maybe_colored_envs}",
            check = "✓".green(),
            lang = "🦀 rust".bold().red(),
            colored_inputs = {
                let mut sorted_build_inputs = self
                    .build_inputs
                    .union(&self.runtime_inputs)
                    .collect::<Vec<_>>();
                sorted_build_inputs.sort();
                sorted_build_inputs.iter().map(|v| v.cyan()).join(", ")
            },
            maybe_colored_envs = {
                if !self.environment_variables.is_empty() {
                    let mut sorted_environment_variables = self
                        .environment_variables
                        .iter()
                        .map(|(k, _)| k)
                        .collect::<Vec<_>>();
                    sorted_environment_variables.sort();
                    format!(
                        " ({})",
                        sorted_environment_variables
                            .iter()
                            .map(|v| v.green())
                            .join(", ")
                    )
                } else {
                    "".to_string()
                }
            }
        );

        Ok(())
    }

    #[tracing::instrument(skip_all, fields(project_dir = %project_dir.display()))]
    async fn add_deps_from_package_json(&mut self, project_dir: &Path) -> color_eyre::Result<()> {
        tracing::debug!("Adding Javascript dependencies...");

        // Infer offline-ness from our stored registry
        // if self.registry.offline() {
        let mut yarn_install_command = Command::new("nix");
        yarn_install_command.args(&["--extra-experimental-features"]);
        yarn_install_command.args(&["flakes nix-command"]);
        yarn_install_command.arg("shell");
        yarn_install_command.arg("nixpkgs#nodejs");
        yarn_install_command.arg("nixpkgs#yarn");
        yarn_install_command.arg("-c");
        yarn_install_command.arg("yarn");
        yarn_install_command.arg("install");

        tracing::trace!(command = ?yarn_install_command.as_std(), "Running");
        let spinner = SimpleSpinner::new_with_message(Some(&format!(
            "Running `{yarn_install}`",
            yarn_install = "nix run nixpkgs#yarn -- install".cyan()
        )))
        .context("Failed to construct progress spinner")?;

        let yarn_install_output = match yarn_install_command.output().await {
            Ok(output) => output,
            Err(err) => {
                let err_msg = format!(
                    "\
                        Could not execute `{yarn_install}`. . Is `{nix}` installed?\n\n\
                        Get instructions for installing Nix: {nix_install_url}\
                        ",
                    yarn_install = "nix run nixpkgs#yarn -- install".cyan(),
                    nix = "nix".cyan(),
                    nix_install_url = "https://nixos.org/download.html".blue().underline(),
                );
                eprintln!("{err_msg}\n\nUnderlying error:\n{err}", err = err.red());
                std::process::exit(1);
            }
        };

        spinner.finish_and_clear();

        if !yarn_install_output.status.success() {
            return Err(eyre!(
                "`nix run nixpkgs#yarn -- install` exited with code {}:\n{}",
                yarn_install_output
                    .status
                    .code()
                    .map(|x| x.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                std::str::from_utf8(&yarn_install_output.stderr)?,
            ));
        }
        // }

        tracing::debug!(fresh = %self.registry.fresh(), "Cache freshness");
        let language_registry = self.registry.language().await.clone();
        language_registry.javascript.default.apply(self);

        let walker = WalkDir::new(&project_dir)
            .follow_links(false)
            .same_file_system(true);

        for entry in walker {
            let entry =
                entry.wrap_err_with(|| eyre!("Could not walk `{}`", project_dir.display()))?;
            if entry.path().components().any(|v| v == Component::Normal("tests".as_ref()) || v == Component::Normal("test".as_ref())) {
                continue;
            }
            if entry.path().components().last() != Some(Component::Normal("package.json".as_ref()))
            {
                continue;
            }
            tracing::trace!(path = %entry.path().display(), "Walking");
            let mut package_json = File::open(entry.path()).await?;
            let package_json_path = entry.path();
            let mut buf = String::default();
            package_json
                .read_to_string(&mut buf)
                .await
                .wrap_err_with(|| eyre!("Could not parse `{}`", package_json_path.display()))?;

            let package_json: PackageJson = serde_json::from_str(&buf).wrap_err_with(|| {
                eyre!("Error parsing `{}` as JSON", package_json_path.display())
            })?;

            let riff_config = package_json.config.and_then(|v| v.riff);

            if let Some(ref name) = &package_json.name {
                if let Some(dep_config) =
                    language_registry.javascript.dependencies.get(name.as_str())
                {
                    tracing::debug!(
                        package_name = %name,
                        "build-inputs" = %dep_config.build_inputs().iter().join(", "),
                        "environment-variables" = %dep_config.environment_variables().iter().map(|(k, v)| format!("{k}={v}")).join(", "),
                        "runtime-inputs" = %dep_config.runtime_inputs().iter().join(", "),
                        "Detected known package information"
                    );
                    dep_config.clone().apply(self);
                }
            }

            if let Some(dep_config) = riff_config {
                tracing::debug!(
                    package = %package_json.name.unwrap_or_default(),
                    "build-inputs" = %dep_config.build_inputs().iter().join(", "),
                    "environment-variables" = %dep_config.environment_variables().iter().map(|(k, v)| format!("{k}={v}")).join(", "),
                    "runtime-inputs" = %dep_config.runtime_inputs().iter().join(", "),
                    "Detected `config.riff` in `package.json`"
                );
                dep_config.apply(self);
            }
        }

        eprintln!(
            "{check} {lang}: {colored_inputs}{maybe_colored_envs}",
            check = "✓".green(),
            lang = "⬢ javascript".bold().green(),
            colored_inputs = {
                let mut sorted_build_inputs = self
                    .build_inputs
                    .union(&self.runtime_inputs)
                    .collect::<Vec<_>>();
                sorted_build_inputs.sort();
                sorted_build_inputs.iter().map(|v| v.cyan()).join(", ")
            },
            maybe_colored_envs = {
                if !self.environment_variables.is_empty() {
                    let mut sorted_environment_variables = self
                        .environment_variables
                        .iter()
                        .map(|(k, _)| k)
                        .collect::<Vec<_>>();
                    sorted_environment_variables.sort();
                    format!(
                        " ({})",
                        sorted_environment_variables
                            .iter()
                            .map(|v| v.green())
                            .join(", ")
                    )
                } else {
                    "".to_string()
                }
            }
        );

        Ok(())
    }
}

pub(crate) trait DevEnvironmentAppliable {
    fn apply(&self, dev_env: &mut DevEnvironment);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs::write;

    #[tokio::test]
    async fn dev_env_to_flake() -> eyre::Result<()> {
        let cache_dir = TempDir::new()?;
        std::env::set_var("XDG_CACHE_HOME", cache_dir.path());
        let registry = DependencyRegistry::new(true).await?;
        let dev_env = DevEnvironment {
            build_inputs: ["cargo", "hello"]
                .into_iter()
                .map(ToString::to_string)
                .collect(),
            environment_variables: [("HELLO", "WORLD"), ("GOODBYE", "WORLD")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            runtime_inputs: ["nix", "libGL"]
                .into_iter()
                .map(ToString::to_string)
                .collect(),
            detected_languages: vec![DetectedLanguage::Rust].into_iter().collect(),
            registry: &registry,
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
        Ok(())
    }

    // This test appears flakey on darwin, occasionally hitting IO errors while writing the
    // Cargo.toml to the temp dir.
    #[tokio::test]
    #[ignore]
    async fn dev_env_detect_supported_project() -> eyre::Result<()> {
        let cache_dir = TempDir::new()?;
        std::env::set_var("XDG_CACHE_HOME", cache_dir.path());
        let temp_dir = TempDir::new().unwrap();
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

[package.metadata.riff]
build-inputs = [ "hello" ]
runtime-inputs = [ "libGL" ]

[package.metadata.riff.environment-variables]
HI = "BYE"

[dependencies]
        "#,
        )
        .await?;

        let registry = DependencyRegistry::new(true).await?;
        let mut dev_env = DevEnvironment::new(&registry);
        let detect = dev_env.detect(temp_dir.path()).await;
        assert!(detect.is_ok(), "{detect:?}");

        assert!(dev_env.build_inputs.get("hello").is_some());
        assert_eq!(
            dev_env.environment_variables.get("HI"),
            Some(&String::from("BYE"))
        );
        assert!(dev_env.runtime_inputs.get("libGL").is_some());
        Ok(())
    }

    #[tokio::test]
    async fn dev_env_detect_unsupported_project() -> eyre::Result<()> {
        let cache_dir = TempDir::new()?;
        std::env::set_var("XDG_CACHE_HOME", cache_dir.path());
        let temp_dir = TempDir::new()?;
        let registry = DependencyRegistry::new(true).await?;
        let mut dev_env = DevEnvironment::new(&registry);
        let detect = dev_env.detect(temp_dir.path()).await;
        assert!(detect.is_err());
        Ok(())
    }
}
