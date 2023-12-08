use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BuildConfig {
    #[serde(default)]
    pub main: Option<GrpBuildConfig>,

    #[serde(default)]
    pub test: Option<GrpBuildConfig>,

    #[serde(default)]
    pub binary_application: Option<GrpBuildConfig>,

    #[serde(default)]
    pub secondary_rules: BTreeMap<String, GrpBuildConfig>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, Copy)]
#[serde(rename_all = "snake_case")]
pub enum TargetNameStrategy {
    /// automatic default
    #[default]
    Auto,
    /// use the file stem (file name without the extension) of the source code
    SourceFileStem,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GrpBuildConfig {
    pub headers: Vec<BuildLoad>,
    pub function_name: String,
    #[serde(default, serialize_with = "crate::serde_helpers::ordered_map")]
    pub extra_key_to_list: HashMap<String, Vec<String>>,
    #[serde(default, serialize_with = "crate::serde_helpers::ordered_map")]
    pub extra_key_to_value: HashMap<String, String>,
    #[serde(default = "default_auto")]
    pub target_name_strategy: TargetNameStrategy,
}

pub fn default_auto() -> TargetNameStrategy {
    TargetNameStrategy::Auto
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, Copy)]
#[serde(rename_all = "snake_case")]
pub enum WriteMode {
    /// automatic default
    #[default]
    Overwrite,
    /// use the file stem (file name without the extension) of the source code
    Append,
}

impl WriteMode {
    pub fn new(append: bool) -> WriteMode {
        if append {
            WriteMode::Append
        } else {
            WriteMode::Overwrite
        }
    }
}
