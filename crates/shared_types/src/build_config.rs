use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BuildConfig {
    #[serde(default)]
    pub main: Option<GrpBuildConfig>,

    #[serde(default)]
    pub test: Option<GrpBuildConfig>,
}

impl BuildConfig {
    pub fn merge(&mut self, other: BuildConfig) {
        match (&mut self.main, other.main) {
            (n @ None, Some(o)) => *n = Some(o),
            (Some(_), Some(_)) => panic!("Unable to merge two specified build configs for main"),
            _ => (),
        };

        match (&mut self.test, other.test) {
            (n @ None, Some(o)) => *n = Some(o),
            (Some(_), Some(_)) => panic!("Unable to merge two specified build configs for test"),
            _ => (),
        };
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BuildLoad {
    pub load_from: String,
    pub load_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GrpBuildConfig {
    pub headers: Vec<BuildLoad>,
    pub function_name: String,
    #[serde(default, serialize_with = "crate::serde_helpers::ordered_map")]
    pub extra_key_to_list: HashMap<String, Vec<String>>,
    #[serde(default, serialize_with = "crate::serde_helpers::ordered_map")]
    pub extra_key_to_value: HashMap<String, String>,
}
