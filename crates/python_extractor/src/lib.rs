use std::{
    collections::{BTreeSet, HashSet},
    io::Read,
    path::PathBuf,
};

use bzl_gen_build_python_utilities::PythonProgram;

use bzl_gen_build_shared_types::api::extracted_data::{DataBlock, ExtractedData};

use anyhow::{Context, Result};

mod extract_py_bzl_gen_build_commands;
mod extract_py_imports;

pub async fn extract_python(
    relative_input_paths: String,
    working_directory: PathBuf,
    output: PathBuf,
    label_or_repo_path: String,
    disable_ref_generation: bool,
    import_path_relative_from: Option<String>,
) -> Result<()> {
    let mut relative_input_paths: Vec<String> =
        if let Some(suffix) = relative_input_paths.strip_prefix('@') {
            std::fs::read_to_string(PathBuf::from(suffix))?
                .lines()
                .map(|e| e.to_string())
                .collect()
        } else {
            vec![relative_input_paths.clone()]
        };

    relative_input_paths.sort();

    let mut data_blocks: Vec<DataBlock> = Default::default();

    for relative_path in relative_input_paths {
        let input_file = working_directory.join(&relative_path);
        let mut refs: HashSet<String> = Default::default();
        let mut defs: BTreeSet<String> = Default::default();
        let mut bzl_gen_build_commands: HashSet<String> = Default::default();

        let mut file = std::fs::File::open(&input_file).with_context(|| {
            format!(
                "While attempting to open file: {:?
    }",
                input_file
            )
        })?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).with_context(|| {
            format!(
                "While attempting to read file: {:?
    }",
                input_file
            )
        })?;

        let input_str = String::from_utf8_lossy(&buffer).into_owned();

        bzl_gen_build_commands.extend(extract_py_bzl_gen_build_commands::extract(
            input_str.as_str(),
        ));

        if !disable_ref_generation {
            let program = PythonProgram::parse(&input_str, "foo.py").with_context(|| {
                format!(
                    "Error while parsing file {:?
        }",
                    input_file
                )
            })?;
            refs.extend(extract_py_imports::extract(&program));
        }

        let file_p = input_file.to_string_lossy();

        if let Some(rel) = import_path_relative_from.as_ref() {
            defs.extend(expand_path_to_defs_from_offset(rel, file_p.as_ref()));
        } else {
            defs.extend(expand_path_to_defs(file_p.as_ref()));
        }
        data_blocks.push(DataBlock {
            entity_path: relative_path,
            defs,
            refs,
            bzl_gen_build_commands,
        })
    }

    let def_refs = ExtractedData {
        label_or_repo_path: label_or_repo_path.clone(),
        data_blocks,
    };

    tokio::fs::write(output, serde_json::to_string_pretty(&def_refs)?).await?;
    Ok(())
}

fn expand_path_to_defs_from_offset(from_given_path: &str, path: &str) -> Vec<String> {
    // rules_python Bzlmod support uses pip-tools, which I think places the 3rdparty
    // source files inside a site-packages/ directory, per module.
    if let Some(rem) = path
        .strip_prefix(from_given_path)
        .and_then(|p| Some(p.strip_prefix("site-packages/").unwrap_or(p)))
    {
        if let Some(e) = rem.strip_suffix(".py") {
            let targ = e.replace('/', ".");

            if let Some(p) = targ.strip_suffix(".__init__") {
                return vec![p.to_string(), targ.clone()];
            } else {
                return vec![targ];
            }
        }
    }
    Vec::default()
}

fn expand_path_to_defs(path: &str) -> Vec<String> {
    let mut results = Vec::default();
    for element in path.split('/') {
        results = results
            .into_iter()
            .map(|r| format!("{}.{}", r, element))
            .collect();

        if element == "src" {
            results.push("src".to_string());
        }
    }

    let mut results: Vec<String> = results
        .into_iter()
        .map(|e| e.strip_suffix(".py").map(|e| e.to_string()).unwrap_or(e))
        .collect();

    results.sort();
    results.dedup();
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_path_to_defs_test() {
        let mut expected = vec!["src.main.python.blah.ppp"];
        expected.sort();
        expected.dedup();

        assert_eq!(
            expand_path_to_defs("/Users/foo/bar/src/main/python/blah/ppp.py"),
            expected
        );
    }

    #[test]
    fn expand_path_to_defs_ambigious_path_test() {
        let mut expected = vec!["src.main.python.blah.src.main.ppp", "src.main.ppp"];
        expected.sort();
        expected.dedup();

        assert_eq!(
            expand_path_to_defs("/Users/foo/bar/src/main/python/blah/src/main/ppp.py"),
            expected
        );
    }

    #[test]
    fn expand_site_packages_path_to_defs_test() {
        let mut expected = vec!["pytz", "pytz.__init__"];
        expected.sort();
        expected.dedup();

        assert_eq!(
            expand_path_to_defs_from_offset("/tmp/aaa/", "/tmp/aaa/site-packages/pytz/__init__.py"),
            expected
        );
    }
}
