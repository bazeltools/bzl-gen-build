use anyhow::{Context, Result};
use bzl_gen_build_python_utilities::PythonProgram;
use bzl_gen_build_shared_types::api::extracted_data::{DataBlock, ExtractedData};
use encoding_rs::*;
use futures::future::join_all;
use lazy_static::lazy_static;
use regex::Regex;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    io::{BufRead, Read},
    path::{Path, PathBuf},
};

mod extract_py_bzl_gen_build_commands;
mod extract_py_imports;

lazy_static! {
    static ref CODECS: HashMap<&'static str, &'static Encoding> = {
        let mut codecs: HashMap<&str, &Encoding> = HashMap::new();
        codecs.insert("BIG5-TW", BIG5);
        codecs.insert("CSBIG5", BIG5);
        codecs.insert("EUCJP", EUC_JP);
        codecs.insert("UJIS", EUC_JP);
        codecs.insert("U-JIS", EUC_JP);
        codecs.insert("EUCKR", EUC_KR);
        codecs.insert("KOREAN", EUC_KR);
        codecs.insert("KSC5601", EUC_KR);
        codecs.insert("KS_C-5601", EUC_KR);
        codecs.insert("KS_C-5601-1987", EUC_KR);
        codecs.insert("KS_X-1001", EUC_KR);
        codecs.insert("EUCKR", EUC_KR);
        codecs.insert("GB18030-2000", GB18030);
        codecs.insert("936", GBK);
        codecs.insert("CP936", GBK);
        codecs.insert("MS936", GBK);
        codecs.insert("IBM866", IBM866);
        codecs.insert("866", IBM866);
        codecs.insert("ISO-2022-JP", ISO_2022_JP);
        codecs.insert("ISO-8859-2", ISO_8859_2);
        codecs.insert("ISO-8859-3", ISO_8859_3);
        codecs.insert("ISO-8859-4", ISO_8859_4);
        codecs.insert("ISO-8859-5", ISO_8859_5);
        codecs.insert("ISO-8859-6", ISO_8859_6);
        codecs.insert("ISO-8859-7", ISO_8859_7);
        codecs.insert("ISO-8859-8", ISO_8859_8);
        codecs.insert("ISO-8859-8-I", ISO_8859_8);
        codecs.insert("ISO-8859-10", ISO_8859_10);
        codecs.insert("ISO-8859-13", ISO_8859_13);
        codecs.insert("ISO-8859-14", ISO_8859_14);
        codecs.insert("ISO-8859-15", ISO_8859_15);
        codecs.insert("ISO-8859-16", ISO_8859_16);
        codecs.insert("MACINTOSH", MACINTOSH);
        codecs.insert("UTF-8", UTF_8);
        codecs.insert("U8", UTF_8);
        codecs.insert("UTF8", UTF_8);
        codecs.insert("UTF-8", UTF_8);
        codecs.insert("U8", UTF_8);
        codecs.insert("UTF8", UTF_8);
        codecs.insert("UTF-16BE", UTF_16BE);
        codecs.insert("UTF-16LE", UTF_16LE);
        codecs.insert("WINDOWS-874", WINDOWS_874);
        codecs.insert("WINDOWS-1250", WINDOWS_1250);
        codecs.insert("WINDOWS-1251", WINDOWS_1251);
        codecs.insert("WINDOWS-1252", WINDOWS_1252);
        codecs.insert("WINDOWS-1253", WINDOWS_1253);
        codecs.insert("WINDOWS-1254", WINDOWS_1254);
        codecs.insert("WINDOWS-1255", WINDOWS_1255);
        codecs.insert("WINDOWS-1256", WINDOWS_1256);
        codecs.insert("WINDOWS-1257", WINDOWS_1257);
        codecs.insert("WINDOWS-1258", WINDOWS_1258);
        codecs
    };
}

pub fn read_file_to_str(input_file: &Path) -> Result<String, anyhow::Error> {
    let mut file = std::fs::File::open(input_file).with_context(|| {
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

    let mut reader = std::io::BufReader::new(&buffer[..]);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).with_context(|| {
        format!(
            "While attempting to read first line from file to check for encoding: {:?
            }",
            input_file
        )
    })?;

    let re = Regex::new(r".*#.*\bcoding[=:]\s*([-\w.]+)")?;
    let encoding = match re.captures(&first_line) {
        Some(caps) => {
            let capture = caps
                .get(1)
                .with_context(|| "While attempting to read encoding alias")?;
            let alias = capture.as_str().trim().to_uppercase();
            CODECS.get(alias.as_str()).unwrap_or(&UTF_8)
        }
        None => UTF_8,
    };

    match encoding {
        enc if enc == UTF_8 => Ok(String::from_utf8_lossy(&buffer).into_owned()),
        _ => {
            let (cow, _, _) = encoding.decode(&buffer);
            Ok(cow.to_string())
        }
    }
}

async fn extract_file(
    import_path_relative_from: &Option<String>,
    working_directory: &Path,
    relative_path: String,
    disable_ref_generation: bool,
) -> Result<DataBlock> {
    let input_file = working_directory.join(&relative_path);
    let mut refs: HashSet<String> = Default::default();
    let mut defs: BTreeSet<String> = Default::default();
    let mut bzl_gen_build_commands: HashSet<String> = Default::default();

    let input_str = read_file_to_str(&input_file)?;

    bzl_gen_build_commands.extend(extract_py_bzl_gen_build_commands::extract(&input_str));

    let file_p = input_file.to_string_lossy();
    if !disable_ref_generation {
        let program = PythonProgram::parse(&input_str, &file_p).with_context(|| {
            format!(
                "Error while parsing file {:?
  }",
                input_file
            )
        })?;
        refs.extend(extract_py_imports::extract(&program));
    }

    let expanded = if let Some(rel) = import_path_relative_from {
        expand_path_to_defs_from_offset(rel, &file_p)
    } else {
        expand_path_to_defs(&relative_path, &file_p)
    };
    defs.extend(expanded);

    Ok(DataBlock {
        entity_path: relative_path,
        defs,
        refs,
        bzl_gen_build_commands,
    })
}

fn split_inputs(relative_input_paths: String) -> Result<Vec<String>> {
    let mut relative_input_paths: Vec<String> =
        if let Some(suffix) = relative_input_paths.strip_prefix('@') {
            std::fs::read_to_string(PathBuf::from(suffix))?
                .lines()
                .map(|e| e.to_string())
                .collect()
        } else {
            // TODO: I think the spec is actually that this should split on ,
            vec![relative_input_paths]
        };

    relative_input_paths.sort();

    Ok(relative_input_paths)
}

pub async fn extract_python(
    relative_input_paths: String,
    working_directory: PathBuf,
    output: PathBuf,
    label_or_repo_path: String,
    disable_ref_generation: bool,
    import_path_relative_from: Option<String>,
) -> Result<()> {
    let relative_input_paths: Vec<String> = split_inputs(relative_input_paths)?;
    let mut data_blocks: Vec<DataBlock> = Vec::with_capacity(relative_input_paths.len());
    let all_loads = relative_input_paths.into_iter().map(|relative_path| {
        extract_file(
            &import_path_relative_from,
            &working_directory,
            relative_path,
            disable_ref_generation,
        )
    });

    for data_block in join_all(all_loads).await {
        data_blocks.push(data_block?);
    }

    let def_refs = ExtractedData {
        label_or_repo_path: label_or_repo_path,
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

fn expand_path_to_defs(relative_path: &str, path: &str) -> Vec<String> {
    let mut results = Vec::default();
    let relative_elements: Vec<&str> = relative_path.split('/').collect();
    for element in path.split('/') {
        results = results
            .into_iter()
            .map(|r| format!("{}.{}", r, element))
            .collect();

        if element == relative_elements[0] {
            results.push(element.to_string());
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

        assert_eq!(
            expand_path_to_defs(
                "src/main/python/blah/ppp.py",
                "/Users/foo/bar/src/main/python/blah/ppp.py"
            ),
            expected
        );
    }

    #[test]
    fn expand_com_path_to_defs_test() {
        let mut expected = vec!["com.blah.ppp"];
        expected.sort();

        assert_eq!(
            expand_path_to_defs("com/blah/ppp.py", "/Users/foo/bar/com/blah/ppp.py"),
            expected
        );
    }

    #[test]
    fn expand_path_to_defs_ambigious_path_test() {
        let mut expected = vec!["src.main.python.blah.src.main.ppp", "src.main.ppp"];
        expected.sort();

        assert_eq!(
            expand_path_to_defs(
                "src/main/python/blah/src/main/ppp.py",
                "/Users/foo/bar/src/main/python/blah/src/main/ppp.py"
            ),
            expected
        );
    }

    #[test]
    fn expand_site_packages_path_to_defs_test() {
        let mut expected = vec!["pytz", "pytz.__init__"];
        expected.sort();

        assert_eq!(
            expand_path_to_defs_from_offset("/tmp/aaa/", "/tmp/aaa/site-packages/pytz/__init__.py"),
            expected
        );
    }
}
