use crate::errors::JarscannerError;

use std::collections::{HashMap, HashSet};

use std::fs::File;
use std::io::Write;
use std::io::{BufReader, BufWriter};
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

fn file_name_to_class_names(
    file_name_ref: &str,
    result: &mut HashSet<String>,
) {
    if non_anon(file_name_ref) && not_in_meta(file_name_ref) && ends_in_class(file_name_ref) {
        let length_of_name = file_name_ref.len();
        let mut file_name_res = String::with_capacity(length_of_name);
        let mut was_slash = false;
        let mut idx = 0;
        // We know this ends in .class, so we're going to need to track the index of the last 6 chars
        let end_idx = length_of_name - 6;
        for file_char in file_name_ref.chars() {
            if idx == end_idx {
                break;
            }

            match file_char {
                '/' => {
                    was_slash = true;
                    file_name_res.push('.')
                }
                '$' => {
                    if idx == end_idx - 1 {
                        // Do nothing
                    } else if !was_slash {
                        was_slash = false;
                        file_name_res.push('.')
                    }
                }
                _ => {
                    was_slash = false;
                    file_name_res.push(file_char)
                }
            }
            idx += 1;
        }

        if file_name_res.contains(".package") {
            result.insert(file_name_res.clone());
            result.insert(file_name_res.replace(".package", ""));
        } else {
            result.insert(file_name_res);
        }
    }
}

fn read_zip_archive(input_jar: &PathBuf) -> Result<HashSet<String>, JarscannerError> {
    let file = File::open(input_jar)?;
    let reader = BufReader::with_capacity(32000, file);
    let archive = ZipArchive::new(reader)?;

    let mut result = HashSet::new();
    for file_name in archive.file_names() {
        file_name_to_class_names(file_name, &mut result);
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
) -> Result<ExtractedData, JarscannerError> {
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
) -> Result<(), JarscannerError> {
    // Note: we tried using to_writer_pretty here, and it was slower!
    let json = serde_json::to_string_pretty(target_descriptor)?;
    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(json.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_path_to_jar() {
        let empty_vec: Vec<&str> = vec![];
        let mut set = HashSet::new();

        // Files that should be ignored
        file_name_to_class_names("META-INF/services/java.time.chrono.Chronology", &mut set);
        assert_eq!(set.iter().collect::<Vec<_>>(), empty_vec);

        // Make sure we're respecting that slash
        file_name_to_class_names(
            "META-INFO/services/java.time.chrono.Chronology.class",
            &mut set,
        );
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            vec!["META-INFO.services.java.time.chrono.Chronology"]
        );

        set.clear();
        file_name_to_class_names("foo/bar/baz/doo.txt", &mut set);
        assert_eq!(set.iter().collect::<Vec<_>>(), empty_vec);

        // Anon classes that should be ignored
        set.clear();
        file_name_to_class_names(
            "scala/util/matching/Regex$$anonfun$replaceSomeIn$1.class",
            &mut set,
        );
        assert_eq!(set.iter().collect::<Vec<_>>(), empty_vec);

        set.clear();
        file_name_to_class_names(
            "autovalue/shaded/com/google$/common/reflect/$Reflection.class",
            &mut set,
        );
        assert_eq!(set.iter().collect::<Vec<_>>(), empty_vec);

        // We should pick up classes
        set.clear();
        file_name_to_class_names(
            "software/amazon/eventstream/HeaderValue$LongValue.class",
            &mut set,
        );
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            vec!["software.amazon.eventstream.HeaderValue.LongValue"]
        );

        // We should pick up package objects
        set.clear();
        file_name_to_class_names("scala/runtime/package$.class", &mut set);
        let mut vec_expected = set.iter().collect::<Vec<_>>();
        vec_expected.sort();
        assert_eq!(vec_expected, vec!["scala.runtime", "scala.runtime.package"]);

        set.clear();
        file_name_to_class_names("scala/runtime/package.class", &mut set);
        let mut vec_expected = set.iter().collect::<Vec<_>>();
        vec_expected.sort();
        assert_eq!(vec_expected, vec!["scala.runtime", "scala.runtime.package"]);
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
