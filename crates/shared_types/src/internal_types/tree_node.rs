use std::collections::{HashSet, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    directive::{
        AttrStringListConfig, BinaryRefAndPath, EntityDirectiveConfig, ManualRefConfig,
        SrcDirectiveConfig,
    },
    Directive,
};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TreeNode {
    pub label_or_repo_path: String,

    pub defs: BTreeSet<String>,

    #[serde(serialize_with = "crate::serde_helpers::ordered_set")]
    pub refs: HashSet<String>,

    #[serde(serialize_with = "crate::serde_helpers::ordered_set")]
    pub runtime_refs: HashSet<String>,

    #[serde(default, serialize_with = "crate::serde_helpers::ordered_list")]
    pub entity_directives: Vec<EntityDirectiveConfig>,

    #[serde(default, serialize_with = "crate::serde_helpers::ordered_list")]
    pub manual_ref_directives: Vec<ManualRefConfig>,

    #[serde(default, serialize_with = "crate::serde_helpers::ordered_list")]
    pub binary_ref_directives: Vec<BinaryRefAndPath>,

    #[serde(default, serialize_with = "crate::serde_helpers::ordered_list")]
    pub attr_string_list_directives: Vec<AttrStringListConfig>,
}

impl TryFrom<crate::api::extracted_data::DataBlock> for TreeNode {
    type Error = anyhow::Error;

    fn try_from(value: crate::api::extracted_data::DataBlock) -> Result<Self, Self::Error> {
        let directives = Directive::from_strings(&value.bzl_gen_build_commands)?;
        let mut t = Self {
            label_or_repo_path: String::default(),
            defs: value.defs,
            refs: value.refs,
            ..Default::default()
        };

        t.apply_directives_with_path(&directives, Some(value.entity_path.as_str()));

        Ok(t)
    }
}

impl TreeNode {
    pub fn from_label(label: String) -> TreeNode {
        TreeNode {
            label_or_repo_path: label,
            ..Default::default()
        }
    }

    fn apply_directives_with_path<'a, T>(&mut self, directives: T, entity_path: Option<&str>)
    where
        T: IntoIterator<Item = &'a Directive> + Copy + std::fmt::Debug,
    {
        for directive in directives.into_iter() {
            match directive {
                Directive::SrcDirective(SrcDirectiveConfig { command, act_on }) => {
                    match command {
                        crate::SrcDirective::Ref => self.refs.insert(act_on.clone()),
                        crate::SrcDirective::Unref => self.refs.remove(act_on),
                        crate::SrcDirective::Def => self.defs.insert(act_on.clone()),
                        crate::SrcDirective::Undef => self.defs.remove(act_on),
                        crate::SrcDirective::RuntimeRef => self.runtime_refs.insert(act_on.clone()),
                        crate::SrcDirective::RuntimeUnref => self.runtime_refs.remove(act_on),
                    };
                }
                Directive::EntityDirective(ed) => self.entity_directives.push(ed.clone()),
                Directive::ManualRef(mr) => self.manual_ref_directives.push(mr.clone()),
                Directive::BinaryRef(mr) => self.binary_ref_directives.push(BinaryRefAndPath {
                    entity_path: entity_path.map(|e| e.to_string()),
                    binary_refs: mr.clone(),
                }),
                Directive::AttrStringList(attr) => {
                    self.attr_string_list_directives.push(attr.clone())
                }
            }
        }
        self.entity_directives.sort();
        self.entity_directives.dedup();

        self.binary_ref_directives.sort();
        self.binary_ref_directives.dedup();

        self.manual_ref_directives.sort();
        self.manual_ref_directives.dedup();

        self.attr_string_list_directives.sort();
        self.attr_string_list_directives.dedup();
    }

    pub fn apply_directives<'a, T>(&mut self, directives: T)
    where
        T: IntoIterator<Item = &'a Directive> + Copy + std::fmt::Debug,
    {
        self.apply_directives_with_path(directives, None);
    }

    pub fn merge(&mut self, mut other: TreeNode) {
        self.defs
            .extend(std::mem::take(&mut other.defs).into_iter());

        self.refs
            .extend(std::mem::take(&mut other.refs).into_iter());

        self.runtime_refs
            .extend(std::mem::take(&mut other.runtime_refs).into_iter());

        self.entity_directives
            .extend(std::mem::take(&mut other.entity_directives).into_iter());
        self.entity_directives.sort();
        self.entity_directives.dedup();

        self.binary_ref_directives
            .extend(std::mem::take(&mut other.binary_ref_directives).into_iter());
        self.binary_ref_directives.sort();
        self.binary_ref_directives.dedup();

        self.manual_ref_directives
            .extend(std::mem::take(&mut other.manual_ref_directives).into_iter());
        self.manual_ref_directives.sort();
        self.manual_ref_directives.dedup();

        self.attr_string_list_directives
            .extend(std::mem::take(&mut other.attr_string_list_directives).into_iter());
        self.attr_string_list_directives.sort();
        self.attr_string_list_directives.dedup();
    }
}
