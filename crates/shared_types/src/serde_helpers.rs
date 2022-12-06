use std::collections::{HashMap, HashSet};

use serde::Serialize;

// From a stackoverflow comment
pub fn ordered_map<S, K, V>(value: &HashMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    K: Serialize + std::hash::Hash + Eq + PartialOrd + Ord,
    V: Serialize,
{
    let ordered: std::collections::BTreeMap<_, _> = value.iter().collect();
    ordered.serialize(serializer)
}

pub fn ordered_set<S, U>(value: &HashSet<U>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    U: Serialize + std::hash::Hash + Eq + PartialOrd + Ord,
{
    let mut ordered: Vec<&U> = value.iter().collect();
    ordered.sort();
    ordered.serialize(serializer)
}

pub fn ordered_list<S, U>(value: &Vec<U>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    U: Serialize + Eq + PartialOrd + Ord,
{
    let mut ordered: Vec<&U> = value.iter().collect();
    ordered.sort();
    ordered.serialize(serializer)
}
