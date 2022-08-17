use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::dev_env::DevEnvironment;

/// A registry of known mappings from language specific dependencies to fsm settings
#[derive(Deserialize, Default, Clone)]
pub struct DependencyRegistry {
    version: usize, // Checked for ABI compat
    #[serde(default)]
    pub(crate) language_rust: RustDependencyRegistry,
}
/// A language specific registry of dependencies to fsm settings
#[derive(Deserialize, Default, Clone)]
pub struct RustDependencyRegistry {
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
    #[serde(default)]
    pub(crate) build_inputs: HashSet<String>,
    /// Any packaging specific environment variables that need to be set
    #[serde(default)]
    pub(crate) environment_variables: HashMap<String, String>,
    /// The Nix packages which should have the result of `lib.getLib` run on them placed on the `LD_LIBRARY_PATH`
    #[serde(default)]
    pub(crate) ld_library_path_inputs: HashSet<String>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum TryApplyError {
    #[error("Duplicate environment variable `{0}`")]
    DuplicateEnvironmentVariables(String),
}

impl RustDependencyConfiguration {
    pub(crate) fn try_apply(self, dev_env: &mut DevEnvironment) -> Result<(), TryApplyError> {
        dev_env.build_inputs = dev_env.build_inputs.union(&self.build_inputs).cloned().collect();
        for (ref env_key, ref env_val) in self.environment_variables {
            if let Some(_) = dev_env.environment_variables.insert(env_key.clone(), env_val.clone()) {
                return Err(TryApplyError::DuplicateEnvironmentVariables(env_key.clone()))
            }
        }
        dev_env.ld_library_path = dev_env.ld_library_path.union(&self.ld_library_path_inputs).cloned().collect();
        Ok(())
    }
}