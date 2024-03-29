use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use super::sha256_value::Sha256Value;
use crate::{
    async_read_json_file, async_write_json_file, to_directory, write_json_file, Extract, Opt,
};
use anyhow::{anyhow, Context, Result};
use bzl_gen_build_shared_types::{
    api::extracted_data::ExtractedData, build_config::SourceConfig,
    internal_types::tree_node::TreeNode, module_config::ModuleConfig, Directive, ProjectConf,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::{DirEntry, WalkBuilder};
use log::info;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtractedMapping {
    pub path: String,
    pub content_sha: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtractedMappings {
    #[serde(default, serialize_with = "crate::serde_helpers::ordered_map")]
    pub relative_path_to_extractmapping: HashMap<String, ExtractedMapping>,
}

lazy_static::lazy_static! {
    static ref SCALA_EXTENSION: std::ffi::OsString = std::ffi::OsString::from("scala");
    static ref JAVA_EXTENSION: std::ffi::OsString = std::ffi::OsString::from("java");
    static ref PYTHON_EXTENSION: std::ffi::OsString = std::ffi::OsString::from("py");
}

#[derive(Debug, Clone)]
pub struct ProcessedFile {
    file_path: PathBuf,
    sha256: Sha256Value,
    extract_path: PathBuf,
}

#[derive(Debug)]
pub struct Extractors(pub HashMap<String, Extractor>);

#[derive(Debug, Clone)]
pub struct Extractor {
    pub path: PathBuf,
    pub extractor_sha: Sha256Value,
}

#[derive(Debug)]
pub struct ExtractConfig {
    extractor: Extractor,
    sha_to_extract_root: PathBuf,
    module_config: &'static ModuleConfig,
    file_extensions: Vec<OsString>,
}

async fn process_file(
    relative_path: PathBuf,
    working_directory: &'static PathBuf,
    path: PathBuf,
    concurrent_io_operations: &'static Semaphore,
    opt: Arc<ExtractConfig>,
) -> Result<(ProcessedFile, Duration)> {
    let _c = concurrent_io_operations.acquire().await?;
    let sha256 = {
        let r = Sha256Value::from_path(path.as_path()).await.map_err(|e| {
            anyhow!(
                "Unable to convert path to sha256 for path {:?} with error {:?}",
                path,
                e
            )
        })?;
        use std::os::unix::ffi::OsStrExt;
        // The input of the relative path is carried through into the output result
        // so we need to include this in our sha we use to identify the file.
        //
        // We also include the sha of the extractor itself
        //
        Sha256Value::hash_iter_bytes(
            vec![
                r.as_bytes(),
                opt.extractor.extractor_sha.as_bytes(),
                relative_path.as_os_str().as_bytes(),
            ]
            .into_iter(),
        )
    };
    let st = Instant::now();

    let processed_file = ProcessedFile {
        file_path: path,
        sha256,
        extract_path: opt.sha_to_extract_root.join(format!("{}", sha256)),
    };

    if processed_file.extract_path.exists() {
        Ok((processed_file, st.elapsed()))
    } else {
        use tokio::process::Command;
        let mut command = Command::new(opt.extractor.path.as_path());
        command
            .arg("--relative-input-paths")
            .arg(relative_path.as_path());
        command
            .arg("--working-directory")
            .arg(working_directory.as_path());
        // This is the same as the relative input path above in this caller invokation
        // but from other ways of outputting this data the two can diverge. For external dependencies
        // this one contains the label, and the other is the only encoding of the path to the file.
        command
            .arg("--label-or-repo-path")
            .arg(relative_path.as_path());
        command
            .arg("--output")
            .arg(processed_file.extract_path.as_path());
        command.kill_on_drop(true);
        let status = {
            let mut spawned_child = command.spawn()?;
            let status = spawned_child.wait().await?;
            status
        };

        if !status.success() {
            return Err(anyhow!("Failed to run program {:#?}", command));
        }
        if !processed_file.extract_path.exists() {
            return Err(anyhow!(
                "Ran sub process but the cache path doesn't exist still, expected it at {:?}",
                processed_file.extract_path
            ));
        }
        Ok((processed_file, st.elapsed()))
    }
}

// check that a file entry is a match
fn has_good_extension(entry: &DirEntry, file_extensions: &Vec<OsString>) -> bool {
    let entry_is_file = entry.file_type().map(|e| e.is_file()).unwrap_or(false);
    entry_is_file && {
        if let Some(ext) = entry.path().extension() {
            let ext_match = file_extensions.iter().any(|e| e == ext);
            ext_match
        } else {
            false
        }
    }
}

pub fn to_globset(test_globs: &Vec<String>) -> Result<GlobSet> {
    let globset_builder = &mut GlobSetBuilder::new();
    if test_globs.is_empty() {
        Ok(globset_builder.add(Glob::new("**/*.*")?).build()?)
    } else {
        Ok(test_globs
            .into_iter()
            .fold(
                Ok(globset_builder),
                |builder: Result<&mut GlobSetBuilder, globset::Error>, glob| {
                    builder.and_then(|b| Ok(b.add(Glob::new(glob.as_str())?)))
                },
            )?
            .build()?)
    }
}

pub fn path_is_match(
    path: &Path,
    test_globs: &Vec<String>,
    test_globset: &GlobSet,
    source_config: &SourceConfig,
) -> bool {
    let is_empty_globs = test_globs.is_empty();
    is_empty_globs || {
        // Path is a match if the path matches the test_globset, and it's SourceConfig::Test,
        // or the patches does NOT match the test_globset, and it's SourceConfig::Main.
        test_globset.is_match(path) ^ (source_config == &SourceConfig::Main)
    }
}

async fn walk_directories<A, F, R>(
    working_directory: &PathBuf,
    child_path: String,
    file_extensions: &Vec<OsString>,
    test_globs: &Vec<String>,
    source_config: SourceConfig,
    extract_config: Option<Arc<ExtractConfig>>,
    on_entry: F,
) -> Result<Vec<A>>
where
    F: Fn(DirEntry, PathBuf, Option<Arc<ExtractConfig>>) -> R,
    R: futures::Future<Output = Result<A>> + Send + 'static,
{
    let mut results: Vec<A> = Vec::default();
    let globset = to_globset(test_globs)?;
    for entry in WalkBuilder::new(working_directory.join(child_path))
        .build()
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| has_good_extension(e, file_extensions))
        .filter(|e| path_is_match(e.path(), test_globs, &globset, &source_config))
    {
        let relative_path = entry
            .path()
            .strip_prefix(working_directory.as_path())?
            .to_path_buf();
        results.push(on_entry(entry, relative_path, extract_config.clone()).await?);
    }
    Ok(results)
}

async fn async_extract_def_refs(
    working_directory: &'static PathBuf,
    child_path: String,
    concurrent_io_operations: &'static Semaphore,
    opt: Arc<ExtractConfig>,
    source_config: SourceConfig,
) -> Result<((PathBuf, Duration), Vec<ProcessedFile>)> {
    let test_globs = &opt.module_config.test_globs;
    let file_extensions = &opt.file_extensions;
    let results = walk_directories(
        working_directory,
        child_path,
        &file_extensions,
        test_globs,
        source_config,
        Some(opt.clone()),
        |entry, relative_path, opt_config| async move {
            match opt_config {
                Some(config) => Ok(tokio::spawn(process_file(
                    relative_path,
                    working_directory,
                    entry.into_path(),
                    concurrent_io_operations,
                    config,
                ))),
                None => Err(anyhow::anyhow!("ExtractConfig not found")),
            }
        },
    )
    .await?;

    let mut max_duration = Duration::ZERO;
    let mut max_target = PathBuf::from("");
    let mut processed_files = Vec::default();
    for r in results {
        let (e, dur) = r.await??;
        if dur > max_duration {
            max_duration = dur;
            max_target = e.file_path.clone();
        }
        processed_files.push(e);
    }
    Ok(((max_target, max_duration), processed_files))
}

async fn merge_defrefs(
    concurrent_io_operations: &Semaphore,
    path_sha_to_merged_defrefs: &'static Path,
    entry: String,
    project_conf: &'static ProjectConf,
    mut work_items: Vec<ProcessedFile>,
    sha_of_conf_config: Arc<String>,
    no_aggregate_source: bool,
) -> Result<(String, ExtractedMapping)> {
    work_items.sort_by(|a, b| a.sha256.cmp(&b.sha256));

    let merged_sha = Sha256Value::hash_iter_bytes(
        work_items
            .iter()
            .map(|e| e.sha256.as_bytes())
            .chain(std::iter::once(sha_of_conf_config.as_bytes()))
            // We need to break the cache if no_aggregate_source changes
            .chain(std::iter::once(if no_aggregate_source {
                &[1][0..1]
            } else {
                &[0][0..1]
            })),
    );

    let treenode_path = path_sha_to_merged_defrefs.join(format!("{}.treenode", merged_sha));

    if !treenode_path.exists() {
        let mut existing: TreeNode = TreeNode::from_label(entry.clone());
        let c = concurrent_io_operations.acquire().await?;

        for ele in work_items.iter() {
            let d: ExtractedData = async_read_json_file(PathBuf::from(&ele.extract_path).as_path())
                .await
                .with_context(|| format!("Was attempting to read file data: {:#?}", ele))?;

            if existing.label_or_repo_path.starts_with("sha256__") {
                existing.label_or_repo_path = d.label_or_repo_path;
            }
            for ele in d.data_blocks {
                let tn: TreeNode = ele.try_into()?;
                existing.merge(tn);
            }
        }

        let directive_strings: Vec<String> = project_conf
            .path_directives
            .iter()
            .filter(|directive| entry.starts_with(&directive.prefix))
            .flat_map(|e| e.directive_strings.iter())
            .cloned()
            .collect();

        let directives = Directive::from_strings(&directive_strings)?;
        existing.apply_directives(&directives);

        async_write_json_file(&treenode_path, &existing).await?;
        drop(c);
    };

    Ok((
        entry,
        ExtractedMapping {
            path: treenode_path.to_string_lossy().to_string(),
            content_sha: format!("{}", merged_sha),
        },
    ))
}

fn extract_configs(
    _opt: &'static Opt,
    project_conf: &'static ProjectConf,
    sha_to_extract_root: &Path,
    extractors: &Extractors,
) -> Result<Vec<ExtractConfig>> {
    let mut cfgs: Vec<ExtractConfig> = Vec::default();
    for (conf_key, v) in project_conf.configurations.iter() {
        let k: &str = conf_key.as_ref();
        let extractor = if let Some(ex) = extractors.0.get(k) {
            ex.clone()
        } else {
            return Err(anyhow!(
                "Missing command line extractor for configuration: {}",
                conf_key
            ));
        };
        let os_string_file_extensions: Vec<OsString> =
            v.file_extensions.iter().map(|ex| ex.into()).collect();

        cfgs.push(ExtractConfig {
            extractor,
            sha_to_extract_root: sha_to_extract_root.to_path_buf(),
            module_config: v,
            file_extensions: os_string_file_extensions,
        });
    }
    Ok(cfgs)
}

async fn inner_load_external(
    _opt: &'static Opt,
    _extract: &'static Extract,
    project_conf: &'static ProjectConf,
    concurrent_io_operations: &'static Semaphore,
    path: PathBuf,
    path_sha_to_merged_defrefs: &'static Path,
    sha_of_conf_config: Arc<String>,
) -> Result<(String, ExtractedMapping)> {
    let sha256 = {
        let c = concurrent_io_operations.acquire().await?;
        let r = Sha256Value::from_path(path.as_path()).await.map_err(|e| {
            anyhow!(
                "Unable to convert path to sha256 for path {:?} with error {:?}",
                path,
                e
            )
        })?;
        drop(c);
        r
    };
    let processed_file = ProcessedFile {
        file_path: path.clone(),
        sha256,
        extract_path: path,
    };
    let work_items = vec![processed_file];
    merge_defrefs(
        concurrent_io_operations,
        path_sha_to_merged_defrefs,
        format!("sha256__{}", sha256),
        project_conf,
        work_items,
        sha_of_conf_config,
        _opt.no_aggregate_source,
    )
    .await
}

async fn load_external(
    opt: &'static Opt,
    extract: &'static Extract,
    project_conf: &'static ProjectConf,
    concurrent_io_operations: &'static Semaphore,
    external: &PathBuf,
    path_sha_to_merged_defrefs: &'static Path,
    sha_of_conf_config: Arc<String>,
) -> Result<Vec<(String, ExtractedMapping)>> {
    let mut results = Vec::default();
    for entry in WalkBuilder::new(external)
        .build()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().map(|e| e.is_file()).unwrap_or(false) {
            let path = entry.into_path();

            let sha_of_conf_config = sha_of_conf_config.clone();
            results.push(tokio::spawn(inner_load_external(
                opt,
                extract,
                project_conf,
                concurrent_io_operations,
                path,
                path_sha_to_merged_defrefs,
                sha_of_conf_config,
            )));
        }
    }
    let mut res2 = Vec::default();
    for r in results {
        let e = r.await??;
        res2.push(e);
    }

    Ok(res2)
}

async fn run_extractors_on_data<'a>(
    opt: &'static Opt,
    project_conf: &'static ProjectConf,
    concurrent_io_operations: &'static Semaphore,
    sha_to_extract_root: &'a Path,
    extractors: &'a Extractors,
) -> Result<(Vec<Vec<ProcessedFile>>, (PathBuf, Duration))> {
    let cfgs: Vec<ExtractConfig> =
        extract_configs(opt, project_conf, sha_to_extract_root, extractors)?;
    let cfg_refs: Vec<Arc<ExtractConfig>> = cfgs.into_iter().map(|cfg| Arc::new(cfg)).collect();
    let mut all_visiting_paths = Vec::default();
    for cfg in cfg_refs {
        all_visiting_paths.extend(
            cfg.module_config
                .main_roots
                .iter()
                .map(|e| (e.clone(), cfg.clone(), SourceConfig::Main)),
        );
        all_visiting_paths.extend(
            cfg.module_config
                .test_roots
                .iter()
                .map(|e| (e.clone(), cfg.clone(), SourceConfig::Test)),
        );
    }

    let mut async_join_handle: Vec<
        tokio::task::JoinHandle<Result<((PathBuf, Duration), Vec<ProcessedFile>)>>,
    > = Vec::default();
    for (path, extract_config, source_config) in all_visiting_paths.into_iter() {
        async_join_handle.push(tokio::spawn(async_extract_def_refs(
            &opt.working_directory,
            path,
            concurrent_io_operations,
            extract_config.clone(),
            source_config,
        )));
    }

    let mut results: Vec<Vec<ProcessedFile>> = Vec::with_capacity(async_join_handle.len());
    let mut max_duration = Duration::ZERO;
    let mut max_target: PathBuf = PathBuf::from("");

    while let Some(nxt) = async_join_handle.pop() {
        let ((cur_t, dur), files) = nxt.await??;
        if dur > max_duration {
            max_duration = dur;
            max_target = cur_t;
        }
        results.push(files);
    }
    Ok((results, (max_target, max_duration)))
}

async fn load_extractors(extract: &'static Extract) -> Result<Extractors> {
    let mut r = HashMap::default();
    for combo in extract.extractor.iter() {
        let p: Vec<&str> = combo.split(':').collect();
        if p.len() != 2 {
            return Err(anyhow!("Passed in extractor was invalid, saw {} , which doesn't have nme:path , e.g. scala:/tmp/scala-extractor", combo));
        }
        let k = p
            .first()
            .expect("Should be impossible via construction above")
            .trim();
        let p = p
            .get(1)
            .expect("Should be impossible via construction above")
            .trim();

        let pb = PathBuf::from(p);
        if !pb.exists() {
            return Err(anyhow!(
                "Passed in extractor path doesn't exist, saw {:?} from {} which doesn't exist",
                pb,
                combo
            ));
        }

        if !pb.is_file() {
            return Err(anyhow!("Passed in extractor pointed at somethnig that isn't a file, saw {:?} from {} which doesn't exist", pb, combo));
        }

        let extractor_sha = Sha256Value::from_path(&pb).await?;
        r.insert(
            k.to_string(),
            Extractor {
                path: pb,
                extractor_sha,
            },
        );
    }

    Ok(Extractors(r))
}

pub async fn extract_defrefs(
    opt: &'static Opt,
    extract: &'static Extract,
    project_conf: &'static ProjectConf,
    concurrent_io_operations: &'static Semaphore,
) -> Result<()> {
    let merged_config_str = serde_json::to_string(project_conf)?;
    let sha_of_conf: Sha256Value = merged_config_str.as_bytes().into();
    let sha_of_conf_config = Arc::new(format!("{}", sha_of_conf));

    let sha_to_extract_root = Box::leak(Box::new(opt.cache_path.join("sha_to_extract")));
    if !sha_to_extract_root.exists() {
        std::fs::create_dir_all(&sha_to_extract_root)?;
    }

    let path_sha_to_merged_defrefs: &'static Path =
        Box::leak(Box::new(opt.cache_path.join("path_sha_to_merged_defrefs"))).as_path();
    if !path_sha_to_merged_defrefs.exists() {
        std::fs::create_dir_all(&path_sha_to_merged_defrefs)?;
    }
    let st = Instant::now();

    // we use the move here to establish a lifetime for the references that only
    // live for the scope of this await
    let extractors = load_extractors(extract).await?;
    let fut = async move {
        run_extractors_on_data(
            opt,
            project_conf,
            concurrent_io_operations,
            sha_to_extract_root,
            &extractors,
        )
        .await
    };
    let probe_files = tokio::spawn(fut);

    let external_expanded: Vec<(String, ExtractedMapping)> =
        if let Some(external) = &extract.external_generated_root {
            load_external(
                opt,
                extract,
                project_conf,
                concurrent_io_operations,
                external,
                path_sha_to_merged_defrefs,
                sha_of_conf_config.clone(),
            )
            .await?
        } else {
            Vec::default()
        };

    let (expanded, (inner_max_path, inner_max_duration)) = probe_files.await??;
    info!(
        "Extraction phase took: {:?}, longest one {:?} - took: {:#?}",
        st.elapsed(),
        inner_max_path,
        inner_max_duration
    );

    let st = Instant::now();

    let mut merge_work: Vec<_> = Vec::default();
    for processed_files in expanded {
        let mut work: HashMap<String, Vec<ProcessedFile>> = HashMap::default();

        for processed_file in processed_files.into_iter() {
            let rel_path = processed_file
                .file_path
                .strip_prefix(&opt.working_directory)?
                .to_string_lossy()
                .to_string();

            let entry = if !opt.no_aggregate_source {
                to_directory(rel_path)
            } else {
                rel_path
            };
            work.entry(entry).or_default().push(processed_file);
        }

        for (entry, files) in work.into_iter() {
            merge_work.push(tokio::spawn(merge_defrefs(
                concurrent_io_operations,
                path_sha_to_merged_defrefs,
                entry,
                project_conf,
                files,
                sha_of_conf_config.clone(),
                opt.no_aggregate_source,
            )))
        }
    }

    let mut result: HashMap<String, ExtractedMapping> = HashMap::default();
    result.extend(external_expanded);

    while let Some(r) = merge_work.pop() {
        let (k, v) = r.await.map_err(|e| anyhow!("{:#?}", e))??;
        result.insert(k, v);
    }
    info!("Merging operations took: {:?}", st.elapsed());

    let extracted_mappings = ExtractedMappings {
        relative_path_to_extractmapping: result,
    };

    write_json_file(extract.extracted_mappings.as_path(), &extracted_mappings)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_walk_directories_in_com() -> Result<(), Box<dyn std::error::Error>> {
        let working_directory = fs::canonicalize(PathBuf::from("../../example"))?;
        let child_path = "com".to_string();
        let py_exts = vec![std::ffi::OsString::from("py")];
        let test_globs = vec!["**/test*.py".to_string(), "**/*test.py".to_string()];
        let result0 = walk_directories(
            &working_directory,
            child_path.clone(),
            &py_exts,
            &test_globs,
            SourceConfig::Main,
            None,
            |_entry, relative_path, _| async move { Ok(relative_path) },
        )
        .await?;
        assert_eq!(result0, vec![PathBuf::from("com/example/hello.py")]);
        let result2 = walk_directories(
            &working_directory,
            child_path.clone(),
            &py_exts,
            &test_globs,
            SourceConfig::Test,
            None,
            |_entry, relative_path, _| async move { Ok(relative_path) },
        )
        .await?;
        assert_eq!(result2, vec![PathBuf::from("com/example/hello_test.py")]);
        Ok(())
    }
}
