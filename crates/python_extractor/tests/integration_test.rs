use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use tempfile::tempdir;

use bzl_gen_build_shared_types::api::extracted_data::ExtractedData;
use bzl_gen_python_extractor as pe;

#[tokio::test]
async fn process_well_formatted_python_module() {
    let tmpdir = tempdir().expect("Failed to create temp directory");
    let tmp_path = format!(
        "python_extractor_output_well_formatted_{}.json",
        std::time::SystemTime::now().elapsed().unwrap().as_nanos()
    );
    let tmp_json_path = tmpdir.path().join(tmp_path);

    pe::extract_python(
        "test_module.py".to_string(),
        PathBuf::from("tests/data/"),
        tmp_json_path.clone(),
        "@pip".to_string(),
        false,
        None,
    )
    .await
    .unwrap();

    let file = File::open(tmp_json_path).unwrap();
    let reader = BufReader::new(file);
    let data: ExtractedData = serde_json::from_reader(reader).unwrap();

    let expected_data = r#"
        {
            "data_blocks": [
                {
                    "entity_path": "test_module.py",
                    "defs": ["test_module"],
                    "refs": [
                        "binascii",
                        "html",
                        "json",
                        "IPython.testing",
                        "binascii.b2a_base64",
                        "os.path",
                        "pathlib.PurePath",
                        "warnings",
                        "IPython.testing.skipdoctest.skip_doctest",
                        "IPython.utils.py3compat",
                        "IPython",
                        "mimetypes",
                        "IPython.utils",
                        "pathlib.Path",
                        "IPython.testing.skipdoctest",
                        "os",
                        "IPython.utils.py3compat.cast_unicode",
                        "copy.deepcopy",
                        "os.path.splitext",
                        "pathlib",
                        "struct",
                        "copy",
                        "binascii.hexlify"
                    ],
                    "bzl_gen_build_commands": []
                }
            ],
            "label_or_repo_path": "@pip"
        }
        "#;

    let expected: ExtractedData = serde_json::from_str(expected_data).unwrap();
    assert!(expected == data)
}

#[tokio::test]
async fn process_non_utf8_python_module() {
    let tmpdir = tempdir().expect("Failed to create temp directory");
    let tmp_path = format!(
        "python_extractor_output_well_formatted_{}.json",
        std::time::SystemTime::now().elapsed().unwrap().as_nanos()
    );
    let tmp_json_path = tmpdir.path().join(tmp_path);

    pe::extract_python(
        "nonascii.py".to_string(),
        PathBuf::from("tests/data/"),
        tmp_json_path.clone(),
        "@pip".to_string(),
        false,
        None,
    )
    .await
    .unwrap();

    let file = File::open(tmp_json_path).unwrap();
    let reader = BufReader::new(file);
    let data: ExtractedData = serde_json::from_reader(reader).unwrap();

    let expected_data = r#"
        {
            "data_blocks": [
                {
                    "entity_path": "nonascii.py",
                    "defs": ["nonascii"],
                    "refs": [],
                    "bzl_gen_build_commands": []
                }
            ],
            "label_or_repo_path": "@pip"
        }
        "#;

    let expected: ExtractedData = serde_json::from_str(expected_data).unwrap();
    assert!(expected == data)
}
