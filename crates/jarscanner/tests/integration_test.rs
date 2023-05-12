use serde_json;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use tempfile::tempdir;

use bzl_gen_build_shared_types::api::extracted_data::ExtractedData;
use bzl_gen_jarscanner as jarscanner;

#[test]
fn process_jar() {
    let relative_paths = vec![
        "tests/data/scala-parser-combinators_2.11-2.2.0.jar",
        "tests/data/scala-parser-combinators_2.12-2.3.0.jar",
    ];
    let tmpdir = tempdir().expect("Failed to create temp directory");
    for relative_path in &relative_paths {
        let current_dir = env::current_dir().unwrap();
        let jar_path = PathBuf::from(current_dir.join(relative_path));
        let mut label_to_allowed_prefixes = HashMap::new();
        label_to_allowed_prefixes.insert(
            "@jvm__com_netflix_iceberg__bdp_iceberg_spark_2_12//:jar".to_string(),
            vec!["com.netflix.iceberg.".to_string()],
        );

        let label = "integration_test_label";
        let processed_input =
            jarscanner::process_input(label, &jar_path, relative_path, &label_to_allowed_prefixes)
                .unwrap();

        // Round-trip the JSON to a tempfile and then make assertions on the results
        let tmp_path = format!(
            "integration_test_output_{}.json",
            std::time::SystemTime::now().elapsed().unwrap().as_nanos()
        );
        let tmp_json_path = tmpdir.path().join(tmp_path);
        jarscanner::emit_result(&processed_input, &tmp_json_path).unwrap();

        let file = File::open(tmp_json_path).unwrap();
        let reader = BufReader::new(file);
        let data: ExtractedData = serde_json::from_reader(reader).unwrap();
        let data_block = data.data_blocks.get(0).unwrap();

        assert_eq!(data.label_or_repo_path, label);
        assert_eq!(data_block.entity_path, relative_path.to_string());
        assert!(data_block
            .defs
            .contains("scala.util.parsing.combinator.JavaTokenParsers"));
        assert!(data_block
            .defs
            .contains("scala.util.parsing.combinator.token.StdTokens.StringLit"));
        assert!(!data_block
            .defs
            .iter()
            .any(|s| s.contains("anon") | s.contains("META-INF")));
    }
}
