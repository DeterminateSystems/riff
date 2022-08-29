use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::dev_env::{DevEnvironment, DevEnvironmentAppliable, TryApplyError};

/// A language specific registry of dependencies to fsm settings
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
    fn try_apply(&self, dev_env: &mut DevEnvironment) -> Result<(), TryApplyError> {
        self.default.try_apply(dev_env)?;
        let target = format!("{}", target_lexicon::HOST);
        // Importantly: These come after, they are more specific.
        if let Some(target_config) = self.targets.get(&target) {
            target_config.try_apply(dev_env)?;
        }

        Ok(())
    }
}

/// Dependency specific information needed for fsm
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
    fn try_apply(&self, dev_env: &mut DevEnvironment) -> Result<(), TryApplyError> {
        dev_env.build_inputs = dev_env
            .build_inputs
            .union(&self.build_inputs)
            .cloned()
            .collect();
        for (ref env_key, ref env_val) in &self.environment_variables {
            if dev_env
                .environment_variables
                .insert(env_key.to_string(), env_val.to_string())
                .is_some()
            {
                return Err(TryApplyError::DuplicateEnvironmentVariables(
                    env_key.to_string(),
                ));
            }
        }
        dev_env.ld_library_path = dev_env
            .ld_library_path
            .union(&self.runtime_inputs)
            .cloned()
            .collect();
        Ok(())
    }
}
