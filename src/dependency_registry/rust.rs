use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::dev_env::{DevEnvironment, DevEnvironmentAppliable};

/// A language specific registry of dependencies to riff settings
#[derive(Deserialize, Default, Clone, Debug)]
pub struct RustDependencyRegistryData {
    /// Settings which are needed for every instance of this language (Eg `cargo` for Rust)
    pub(crate) default: RustDependencyTargetData,
    /// A mapping of dependencies (by crate name) to configuration
    // TODO(@hoverbear): How do we handle crates with conflicting names? eg a `rocksdb-sys` crate from one repo and another from another having different requirements?
    pub(crate) dependencies: HashMap<String, RustDependencyData>,
}

#[derive(Deserialize, Default, Clone, Debug)]
pub struct RustDependencyData {
    #[serde(flatten)]
    pub(crate) default: RustDependencyTargetData,
    // Keep the key a `String` since users can make custom targets.
    #[serde(default)]
    pub(crate) targets: HashMap<String, RustDependencyTargetData>,
}

impl RustDependencyData {
    #[tracing::instrument(skip_all)]
    pub(crate) fn build_inputs(&self) -> HashSet<String> {
        let target = format!("{}", target_lexicon::HOST);
        let mut build_inputs = self.default.build_inputs.clone();
        // Importantly: These come after, they are more specific.
        if let Some(target_config) = self.targets.get(&target) {
            build_inputs = build_inputs
                .union(&target_config.build_inputs)
                .cloned()
                .collect();
        }
        build_inputs
    }
    #[tracing::instrument(skip_all)]
    pub(crate) fn environment_variables(&self) -> HashMap<String, String> {
        let target = format!("{}", target_lexicon::HOST);
        let mut environment_variables = self.default.environment_variables.clone();
        // Importantly: These come after, they are more specific.
        if let Some(target_config) = self.targets.get(&target) {
            for (k, v) in &target_config.environment_variables {
                environment_variables.insert(k.clone(), v.clone());
            }
        }
        environment_variables
    }
    #[tracing::instrument(skip_all)]
    pub(crate) fn runtime_inputs(&self) -> HashSet<String> {
        let target = format!("{}", target_lexicon::HOST);
        let mut runtime_inputs = self.default.runtime_inputs.clone();
        // Importantly: These come after, they are more specific.
        if let Some(target_config) = self.targets.get(&target) {
            runtime_inputs = runtime_inputs
                .union(&target_config.runtime_inputs)
                .cloned()
                .collect();
        }
        runtime_inputs
    }
}

impl DevEnvironmentAppliable for RustDependencyData {
    #[tracing::instrument(skip_all)]
    fn apply(&self, dev_env: &mut DevEnvironment) {
        self.default.apply(dev_env);
        let target = format!("{}", target_lexicon::HOST);
        // Importantly: These come after, they are more specific.
        if let Some(target_config) = self.targets.get(&target) {
            target_config.apply(dev_env);
        }
    }
}

/// Dependency specific information needed for riff
#[derive(Deserialize, Default, Clone, Debug)]
pub struct RustDependencyTargetData {
    /// The Nix `buildInputs` needed
    #[serde(default, rename = "build-inputs")]
    pub(crate) build_inputs: HashSet<String>,
    /// Any packaging specific environment variables that need to be set
    #[serde(default, rename = "environment-variables")]
    pub(crate) environment_variables: HashMap<String, String>,
    /// The Nix packages which should have the result of `lib.getLib` run on them placed on the `LD_LIBRARY_PATH`
    #[serde(default, rename = "runtime-inputs")]
    pub(crate) runtime_inputs: HashSet<String>,
}

impl DevEnvironmentAppliable for RustDependencyTargetData {
    #[tracing::instrument(skip_all)]
    fn apply(&self, dev_env: &mut DevEnvironment) {
        dev_env.build_inputs = dev_env
            .build_inputs
            .union(&self.build_inputs)
            .cloned()
            .collect();
        for (ref env_key, ref env_val) in &self.environment_variables {
            if let Some(existing_value) = dev_env
                .environment_variables
                .insert(env_key.to_string(), env_val.to_string())
            {
                tracing::debug!(
                    key = env_key,
                    existing_value,
                    new_value = env_val,
                    "Overriding previously declared environment variable"
                )
            }
        }
        dev_env.runtime_inputs = dev_env
            .runtime_inputs
            .union(&self.runtime_inputs)
            .cloned()
            .collect();
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::dependency_registry::DependencyRegistry;
    use tempfile::TempDir;

    #[tokio::test]
    async fn try_apply() -> eyre::Result<()> {
        let cache_dir = TempDir::new()?;
        std::env::set_var("XDG_CACHE_HOME", cache_dir.path());
        let registry = DependencyRegistry::new(true).await?;
        let mut dev_env = DevEnvironment::new(registry);

        let target = format!("{}", target_lexicon::HOST);
        let data = RustDependencyData {
            default: RustDependencyTargetData {
                build_inputs: vec!["default".into()].into_iter().collect(),
                environment_variables: vec![
                    ("DEFAULT_VAR".into(), "default".into()),
                    ("CONFLICT".into(), "default".into()),
                ]
                .into_iter()
                .collect(),
                runtime_inputs: vec!["default".into()].into_iter().collect(),
            },
            targets: {
                let mut map = HashMap::default();
                map.insert(
                    target,
                    RustDependencyTargetData {
                        build_inputs: vec!["target_specific".into()].into_iter().collect(),
                        environment_variables: vec![
                            ("TARGET_VAR".into(), "target_specific".into()),
                            ("CONFLICT".into(), "target_specific".into()),
                        ]
                        .into_iter()
                        .collect(),
                        runtime_inputs: vec!["target_specific".into()].into_iter().collect(),
                    },
                );
                map
            },
        };

        data.apply(&mut dev_env);

        assert_eq!(
            dev_env.build_inputs,
            vec!["default".into(), "target_specific".into()]
                .into_iter()
                .collect()
        );
        assert_eq!(
            dev_env.environment_variables,
            vec![
                ("DEFAULT_VAR".into(), "default".into()),
                ("TARGET_VAR".into(), "target_specific".into()),
                ("CONFLICT".into(), "target_specific".into()),
            ]
            .into_iter()
            .collect()
        );
        assert_eq!(
            dev_env.runtime_inputs,
            vec!["default".into(), "target_specific".into()]
                .into_iter()
                .collect()
        );

        Ok(())
    }

    #[test]
    fn build_input_merge() -> eyre::Result<()> {
        let target = format!("{}", target_lexicon::HOST);
        let data = RustDependencyData {
            default: RustDependencyTargetData {
                build_inputs: vec!["default".into()].into_iter().collect(),
                ..Default::default()
            },
            targets: {
                let mut map = HashMap::default();
                map.insert(
                    target,
                    RustDependencyTargetData {
                        build_inputs: vec!["target_specific".into()].into_iter().collect(),
                        ..Default::default()
                    },
                );
                map
            },
        };
        let merged = data.build_inputs();
        assert_eq!(
            merged,
            vec!["default".into(), "target_specific".into()]
                .into_iter()
                .collect()
        );
        Ok(())
    }

    #[test]
    fn environment_variables_merge() -> eyre::Result<()> {
        let target = format!("{}", target_lexicon::HOST);
        let data = RustDependencyData {
            default: RustDependencyTargetData {
                environment_variables: vec![
                    ("DEFAULT_VAR".into(), "default".into()),
                    ("CONFLICT".into(), "default".into()),
                ]
                .into_iter()
                .collect(),
                ..Default::default()
            },
            targets: {
                let mut map = HashMap::default();
                map.insert(
                    target,
                    RustDependencyTargetData {
                        environment_variables: vec![
                            ("TARGET_VAR".into(), "target_specific".into()),
                            ("CONFLICT".into(), "target_specific".into()),
                        ]
                        .into_iter()
                        .collect(),
                        ..Default::default()
                    },
                );
                map
            },
        };
        let merged = data.environment_variables();
        assert_eq!(
            merged,
            vec![
                ("DEFAULT_VAR".into(), "default".into()),
                ("TARGET_VAR".into(), "target_specific".into()),
                ("CONFLICT".into(), "target_specific".into()),
            ]
            .into_iter()
            .collect()
        );
        Ok(())
    }

    #[test]
    fn runtime_input_merge() -> eyre::Result<()> {
        let target = format!("{}", target_lexicon::HOST);
        let data = RustDependencyData {
            default: RustDependencyTargetData {
                runtime_inputs: vec!["default".into()].into_iter().collect(),
                ..Default::default()
            },
            targets: {
                let mut map = HashMap::default();
                map.insert(
                    target,
                    RustDependencyTargetData {
                        runtime_inputs: vec!["target_specific".into()].into_iter().collect(),
                        ..Default::default()
                    },
                );
                map
            },
        };
        let merged = data.runtime_inputs();
        assert_eq!(
            merged,
            vec!["default".into(), "target_specific".into()]
                .into_iter()
                .collect()
        );
        Ok(())
    }
}
