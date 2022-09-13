use crate::dependency_registry::rust::RustDependencyData;

#[derive(serde::Deserialize)]
pub struct CargoMetadata {
    pub packages: Vec<CargoMetadataPackage>,
}

#[derive(serde::Deserialize)]
pub struct CargoMetadataPackage {
    pub name: String,
    pub metadata: Option<RiffMetadata>,
}

#[derive(serde::Deserialize)]
pub struct RiffMetadata {
    pub riff: Option<RustDependencyData>,
}
