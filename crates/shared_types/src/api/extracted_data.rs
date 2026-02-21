use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractedData {
    /// Extractors (e.g. python-entity-extractor) may omit this when they have no block data.
    #[serde(default)]
    pub data_blocks: Vec<DataBlock>,
    /// Wheel scanner JSON and some extractors may omit this.
    #[serde(default)]
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
