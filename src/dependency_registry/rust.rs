use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::dev_env::{DevEnvironment, DevEnvironmentAppliable, TryApplyError};

/// A language specific registry of dependencies to fsm settings
#[derive(Deserialize, Default, Clone)]
pub struct RustDependencyRegistryData {
    /// Settings which are needed for every instance of this language (Eg `cargo` for Rust)
    #[serde(default)]
    pub(crate) default: RustDependencyConfiguration,
    /// A mapping of dependencies (by crate name) to configuration
    // TODO(@hoverbear): How do we handle crates with conflicting names? eg a `rocksdb-sys` crate from one repo and another from another having different requirements?
    #[serde(default)]
    pub(crate) dependencies: HashMap<String, RustDependencyConfiguration>,
}
/// Dependency specific information needed for fsm
#[derive(Deserialize, Default, Clone)]
pub struct RustDependencyConfiguration {
    /// The Nix `buildInputs` needed
    #[serde(default, rename = "build-inputs")]
    pub(crate) build_inputs: HashSet<String>,
    /// Any packaging specific environment variables that need to be set
    #[serde(default, rename = "environment-variables")]
    pub(crate) environment_variables: HashMap<String, String>,
    /// The Nix packages which should have the result of `lib.getLib` run on them placed on the `LD_LIBRARY_PATH`
    #[serde(default, rename = "ld-library-path-inputs")]
    pub(crate) ld_library_path_inputs: HashSet<String>,
}

impl DevEnvironmentAppliable for RustDependencyConfiguration {
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
            .union(&self.ld_library_path_inputs)
            .cloned()
            .collect();
        Ok(())
    }
}
