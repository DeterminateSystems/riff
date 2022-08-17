use std::collections::HashMap;

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
    pub fsm: Option<Fsm>,
}

#[derive(serde::Deserialize)]
pub struct Fsm {
    #[serde(rename = "build-inputs")]
    pub build_inputs: Option<HashMap<String, String>>,
    #[serde(rename = "environment-variables")]
    pub environment_variables: Option<HashMap<String, String>>,
    #[serde(rename = "LD_LIBRARY_PATH-inputs")]
    pub ld_library_path_inputs: Option<HashMap<String, String>>,
}
