use crate::dependency_registry::javascript::JavascriptDependencyData;

#[derive(serde::Deserialize)]
pub struct PackageJson {
    pub name: String,
    pub config: PackageJsonConfig,
}

#[derive(serde::Deserialize)]
pub struct PackageJsonConfig {
    pub riff: Option<JavascriptDependencyData>,
}
