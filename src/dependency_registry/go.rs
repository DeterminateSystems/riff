use std::collections::HashMap;
use super::rust::RustDependencyData;
use serde::Deserialize;

// Cribbing RustDependencyData here because there's nothing really
// rust-specific about it besides the name.

// Not just reusing RustDependencyRegistryData entirely, because
// there's at least the conceptual difference that the map keys
// are Go package paths and not plain crate URLs
#[derive(Deserialize, Default, Clone, Debug)]
pub struct GoDependencyRegistryData {
    pub(crate) default: RustDependencyData,
    pub(crate) dependencies: HashMap<String, RustDependencyData>,
}
