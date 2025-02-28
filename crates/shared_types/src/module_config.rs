use serde::{Deserialize, Serialize};

use crate::build_config::BuildConfig;

#[derive(Debug, Serialize, Default, Deserialize, PartialEq, Eq)]
pub struct ModuleConfig {
    pub file_extensions: Vec<String>,

    #[serde(default)]
    pub build_config: BuildConfig,

    #[serde(default)]
    pub main_roots: Vec<String>,

    #[serde(default)]
    pub test_roots: Vec<String>,

    #[serde(default)]
    pub test_globs: Vec<String>,

    #[serde(default)]
    pub circular_dependency_allow_list: Vec<String>,
}

impl ModuleConfig {
    pub fn merge(&mut self, other: ModuleConfig) {
        self.build_config.merge(other.build_config);

        self.main_roots.extend(other.main_roots.into_iter());
        self.main_roots.sort();
        self.main_roots.dedup();

        self.test_roots.extend(other.test_roots.into_iter());
        self.test_roots.sort();
        self.test_roots.dedup();
    }
}
