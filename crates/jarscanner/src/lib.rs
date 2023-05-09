use crate::errors::FileNameError;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::error::Error;

use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use zip::read::ZipArchive;

pub mod errors;

#[derive(Serialize, Deserialize)]
struct DataBlock {
    entity_path: String,
    defs: Vec<String>,
    refs: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TargetDescriptor {
    label_or_repo_path: String,
    data_blocks: Vec<DataBlock>,
}

fn non_anon(file_name: &str) -> bool {
    !((file_name.contains("/$") && file_name.contains("$/")) || file_name.contains("$$anon"))
}

fn not_in_meta(file_name: &str) -> bool {
    !file_name.starts_with("META-INF")
}

fn ends_in_class(file_name: &str) -> bool {
    file_name.ends_with(".class")
}

fn file_name_to_class_names(file_name: &str) -> Result<Vec<String>, FileNameError> {
    if non_anon(file_name) && not_in_meta(file_name) && ends_in_class(file_name) {
        let base = file_name
            .strip_suffix(".class")
            .ok_or_else(|| {
                FileNameError::new(format!("Failed to strip .class suffix for {}", file_name))
            })?
            .strip_suffix("$")
            .ok_or_else(|| {
                FileNameError::new(format!("Failed to strip $ suffix for {}", file_name))
            })?;
        let dotted = base.replace("/$", "/").replace("$", ".").replace("/", ".");
        let replace_pkg = dotted.replace(".package", "");
        if dotted.contains(".package") {
            Ok(vec![dotted, replace_pkg])
        } else {
            Ok(vec![dotted])
        }
    } else {
        Ok(vec![])
    }
}

fn read_zip_archive(input_jar: &PathBuf) -> Result<Vec<String>, Box<dyn Error>> {
    let file = File::open(input_jar)?;
    let archive = ZipArchive::new(file)?;

    let mut result = Vec::new();
    for file_name in archive.file_names() {
        match file_name_to_class_names(file_name) {
            Ok(class_names) => result.extend(class_names),
            Err(err) => return Err(Box::new(err)),
        }
    }

    Ok(result)
}

fn filter_prefixes(
    label: &str,
    classes: Vec<String>,
    label_to_allowed_prefixes: &HashMap<String, Vec<String>>,
) -> Vec<String> {
    match label_to_allowed_prefixes.get(label) {
        Some(prefix_vec) => classes
            .into_iter()
            .filter(|c| prefix_vec.iter().any(|prefix| c.starts_with(prefix)))
            .collect(),
        None => classes,
    }
}

fn sort_and_deduplicate(vec: &Vec<String>) -> Vec<String> {
    let mut v = vec.clone();
    v.sort();
    v.dedup();
    v
}

pub fn process_input(
    label: &str,
    input_jar: &PathBuf,
    relative_path: &str,
    label_to_allowed_prefixes: &HashMap<String, Vec<String>>,
) -> Result<TargetDescriptor, Box<dyn Error>> {
    let raw_classes = read_zip_archive(input_jar)?;
    let classes = filter_prefixes(label, raw_classes, &label_to_allowed_prefixes);

    Ok(TargetDescriptor {
        label_or_repo_path: label.to_string(),
        data_blocks: vec![DataBlock {
            entity_path: relative_path.to_string(),
            defs: sort_and_deduplicate(&classes),
            refs: vec![],
        }],
    })
}

pub fn emit_result(
    target_descriptor: &TargetDescriptor,
    output_path: &PathBuf,
) -> Result<(), Box<dyn Error>> {
    let json = serde_json::to_string_pretty(target_descriptor)?;
    let mut file = File::create(output_path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}
