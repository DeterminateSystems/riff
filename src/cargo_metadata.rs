use crate::dependency_registry::rust::RustDependencyConfiguration;

#[derive(serde::Deserialize)]
pub struct CargoMetadata {
    pub packages: Vec<CargoMetadataPackage>,
}

#[derive(serde::Deserialize)]
pub struct CargoMetadataPackage {
    pub name: String,
    pub metadata: Option<FsmMetadata>,
}

#[derive(serde::Deserialize)]
pub struct FsmMetadata {
    pub fsm: Option<RustDependencyConfiguration>,
}
