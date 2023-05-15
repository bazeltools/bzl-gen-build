use crate::errors::FileNameError;

use std::collections::{HashMap, HashSet};
use std::error::Error;

use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use zip::read::ZipArchive;

use bzl_gen_build_shared_types::api::extracted_data::{DataBlock, ExtractedData};

pub mod errors;

// This is to filter Scala-generated classes that shouldn't be referenced by users
fn non_anon(file_name: &str) -> bool {
    !((file_name.contains(r"/$") && file_name.contains(r"$/")) || file_name.contains("$$anon"))
}

fn not_in_meta(file_name: &str) -> bool {
    !file_name.starts_with(r"META-INF/")
}

fn ends_in_class(file_name: &str) -> bool {
    file_name.ends_with(".class")
}

fn file_name_to_class_names(file_name: &str) -> Result<Vec<String>, FileNameError> {
    if non_anon(file_name) && not_in_meta(file_name) && ends_in_class(file_name) {
        let class_suffix = file_name.strip_suffix(".class").ok_or_else(|| {
            FileNameError::new(format!("Failed to strip .class suffix for {}", file_name))
        })?;
        let final_suffix = class_suffix.strip_suffix("$").unwrap_or(class_suffix);

        let dotted = final_suffix
            .replace(r"/$", r"/")
            .replace("$", ".")
            .replace(r"/", ".");
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

fn read_zip_archive(input_jar: &PathBuf) -> Result<HashSet<String>, Box<dyn Error>> {
    let file = File::open(input_jar)?;
    let archive = ZipArchive::new(file)?;

    let mut result = HashSet::new();
    for file_name in archive.file_names() {
        match file_name_to_class_names(file_name) {
            Ok(class_names) => result.extend(class_names.into_iter()),
            Err(err) => return Err(Box::new(err)),
        }
    }

    Ok(result)
}

fn filter_prefixes(
    label: &str,
    classes: HashSet<String>,
    label_to_allowed_prefixes: &HashMap<String, Vec<String>>,
) -> HashSet<String> {
    match label_to_allowed_prefixes.get(label) {
        Some(prefix_set) => classes
            .into_iter()
            .filter(|c| prefix_set.iter().any(|prefix| c.starts_with(prefix)))
            .collect(),
        None => classes,
    }
}

pub fn process_input(
    label: &str,
    input_jar: &PathBuf,
    relative_path: &str,
    label_to_allowed_prefixes: &HashMap<String, Vec<String>>,
) -> Result<ExtractedData, Box<dyn Error>> {
    let raw_classes = read_zip_archive(input_jar)?;
    let classes = filter_prefixes(label, raw_classes, &label_to_allowed_prefixes);

    Ok(ExtractedData {
        label_or_repo_path: label.to_string(),
        data_blocks: vec![DataBlock {
            entity_path: relative_path.to_string(),
            defs: classes,
            refs: HashSet::new(),
            bzl_gen_build_commands: HashSet::new(),
        }],
    })
}

pub fn emit_result(
    target_descriptor: &ExtractedData,
    output_path: &PathBuf,
) -> Result<(), Box<dyn Error>> {
    let json = serde_json::to_string_pretty(target_descriptor)?;
    let mut file = File::create(output_path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_path_to_jar() {
        let empty_vec: Vec<String> = vec![];

        // Files that should be ignored
        assert_eq!(
            file_name_to_class_names("META-INF/services/java.time.chrono.Chronology").unwrap(),
            empty_vec
        );
        // Make sure we're respecting that slash
        assert_eq!(
            file_name_to_class_names("META-INFO/services/java.time.chrono.Chronology.class")
                .unwrap(),
            vec!["META-INFO.services.java.time.chrono.Chronology"]
        );
        assert_eq!(
            file_name_to_class_names("foo/bar/baz/doo.txt").unwrap(),
            empty_vec
        );

        // Anon classes that should be ignored
        assert_eq!(
            file_name_to_class_names("scala/util/matching/Regex$$anonfun$replaceSomeIn$1.class")
                .unwrap(),
            empty_vec
        );
        assert_eq!(
            file_name_to_class_names(
                "autovalue/shaded/com/google$/common/reflect/$Reflection.class"
            )
            .unwrap(),
            empty_vec
        );

        // We should pick up classes
        assert_eq!(
            file_name_to_class_names("software/amazon/eventstream/HeaderValue$LongValue.class")
                .unwrap(),
            vec!["software.amazon.eventstream.HeaderValue.LongValue"]
        );

        // We should pick up package objects
        assert_eq!(
            file_name_to_class_names("scala/runtime/package$.class").unwrap(),
            vec!["scala.runtime.package", "scala.runtime"]
        );
        assert_eq!(
            file_name_to_class_names("scala/runtime/package.class").unwrap(),
            vec!["scala.runtime.package", "scala.runtime"]
        );
    }

    #[test]
    fn test_filter_prefixes() {
        let mut label_to_allowed_prefixes = HashMap::new();
        label_to_allowed_prefixes.insert(
            "@jvm__com_netflix_iceberg__bdp_iceberg_spark_2_12//:jar".to_string(),
            vec!["com.netflix.iceberg.".to_string()],
        );

        let mut classes = HashSet::new();
        classes.insert("bar".to_string());
        let expected = classes.clone();

        assert_eq!(
            filter_prefixes("foo", classes, &label_to_allowed_prefixes),
            expected
        );

        let mut filtered_classes = HashSet::new();
        filtered_classes.insert("com.netflix.iceberg.Foo".to_string());
        filtered_classes.insert("com.google.bar".to_string());

        let mut expected = HashSet::new();
        expected.insert("com.netflix.iceberg.Foo".to_string());

        assert_eq!(
            filter_prefixes(
                "@jvm__com_netflix_iceberg__bdp_iceberg_spark_2_12//:jar",
                filtered_classes,
                &label_to_allowed_prefixes
            ),
            expected
        );
    }
}
