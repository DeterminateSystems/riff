use std::collections::{HashMap, HashSet};

use serde::{Deserialize};

/// A registry of known mappings from language specific dependencies to fsm settings
#[derive(Deserialize, Default)]
pub(crate) struct DependencyRegistry {
    version: usize, // Checked for ABI compat
    #[serde(default)]
    pub(crate) language_rust: RustDependencyRegistry,
}
/// A language specific registry of dependencies to fsm settings
#[derive(Deserialize, Default)]
pub(crate) struct RustDependencyRegistry {
    /// Settings which are needed for every instance of this language (Eg `cargo` for Rust)
    #[serde(default)]
    pub(crate) default: RustDependencyConfiguration,
    /// A mapping of dependencies (by crate name) to configuration
    // TODO(@hoverbear): How do we handle crates with conflicting names? eg a `rocksdb-sys` crate from one repo and another from another having different requirements?
    #[serde(default)]
    pub(crate) dependencies: HashMap<String, RustDependencyConfiguration>,
}
/// Dependency specific information needed for fsm
#[derive(Deserialize, Default)]
pub(crate) struct RustDependencyConfiguration {
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

impl RustDependencyConfiguration {
    fn try_union(&self, other: Self) -> Result<Self, ()> {
        todo!()
    }
}