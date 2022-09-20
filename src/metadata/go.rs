use std::path::PathBuf;

use crate::dependency_registry::rust::RustDependencyData;

#[derive(Debug, serde::Deserialize)]
pub(crate) struct GoPackage {
    #[serde(rename = "Dir")]
    pub(crate) dir: PathBuf,
    #[serde(rename = "ImportPath")]
    pub(crate) import_path: String,
    #[serde(rename = "CgoCFLAGS")]
    pub(crate) cgo_cflags: Option<Vec<String>>,
    #[serde(rename = "CgoLDFLAGS")]
    pub(crate) cgo_ldflags: Option<Vec<String>>,
    #[serde(rename = "CgoPkgConfig")]
    pub(crate) cgo_pkg_config: Option<Vec<String>>,
}
