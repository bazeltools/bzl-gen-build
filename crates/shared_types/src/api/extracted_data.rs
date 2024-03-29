use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractedData {
    pub data_blocks: Vec<DataBlock>,
    pub label_or_repo_path: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataBlock {
    pub entity_path: String,
    pub defs: BTreeSet<String>,
    #[serde(serialize_with = "crate::serde_helpers::ordered_set")]
    pub refs: HashSet<String>,
    #[serde(default, serialize_with = "crate::serde_helpers::ordered_set")]
    pub bzl_gen_build_commands: HashSet<String>,
}
