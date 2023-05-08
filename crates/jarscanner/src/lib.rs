use std::collections::HashMap;
use std::path::PathBuf;
use std::io::prelude::*;
use std::fs::File;
use std::error::Error;
use std::fmt;
use zip::{ZipWriter, CompressionMethod, write::FileOptions};
use zip::read::ZipArchive;
use serde::{Serialize, Deserialize};
use serde_json::Result as SerdeJsonResult;

mod errors;

#[derive(Serialize, Deserialize)]
struct DataBlock {
    entity_path: String,
    defs: Vec<String>,
    refs: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct TargetDescriptor {
    label_or_repo_path: String,
    data_blocks: Vec<DataBlock>,
}

fn non_anon(file_name: &str) -> bool {
    !((file_name.contains("/$") && file_name.contains("$/")) || file_name.contains("$$anon"))
}

fn not_in_meta(file_name: &str) -> bool {
    !file_name.starts_with("META-INF")
}

fn ends_class(file_name: &str) -> bool {
    file_name.ends_with(".class")
}

fn file_name_to_class_names(file_name: &str) -> Result<Vec<String>, FileNameError> {
    if non_anon(file_name) && not_in_meta(file_name) && ends_class(file_name) {
        let base = file_name
            .strip_suffix(".class")
            .ok_or_else(|| FileNameError::new("Failed to strip .class suffix"))?
            .strip_suffix("$")
            .ok_or_else(|| FileNameError::new("Failed to strip $ suffix"))?;
        let dotted = base.replace("/$", "/").replace("$", ".").replace("/", ".");
        if dotted.contains(".package") {
            Ok(vec![dotted.clone(), dotted.replace(".package", "")])
        } else {
            Ok(vec![dotted])
        }
    } else {
        Ok(vec![])
    }
}

// fn filter_prefixes(label: &str, classes: Vec<String>, label_to_allowed_prefixes: &HashMap<String, Vec<String>>) -> Vec<String> {
//     match label_to_allowed_prefixes.get(label) {
//         Some(prefix_list) => {
//             classes.into_iter()
//                 .filter(|c| prefix_list.iter().any(|prefix| c.starts_with(prefix)))
//                 .collect()
//         }
//         None => classes
//     }
// }

// fn process_input(label: &str, input_jar: &PathBuf, relative_path: &str, label_to_allowed_prefixes: &HashMap<String, Vec<String>>) -> Result<TargetDescriptor> {
//     let file = File::open(input_jar).unwrap();
//     let mut archive = ZipArchive::new(file).unwrap();
//     let raw_classes: Vec<String> = archive.file_names()
//         .flat_map(file_name_to_class_names)
//         .collect();

//     let classes = filter_prefixes(label, raw_classes, &label_to_allowed_prefixes);
//     let target_descriptor = TargetDescriptor {
//         label_or_repo_path: label.to_string(),
//         data_blocks: vec![DataBlock {
//             entity_path: relative_path.to_string(),
//             defs: classes.into_iter().sorted().dedup().collect(),
//             refs: vec![],
//         }],
//     };

//     Ok(target_descriptor)
// }

// fn emit_result(target_descriptor: &TargetDescriptor, output_path: &PathBuf) -> Result<()> {
//     let json = serde_json::to_string_pretty(target_descriptor)?;
//     let mut file = File::create(output_path)?;
//     file.write_all(json.as_bytes())?;
//     Ok(())
// }
