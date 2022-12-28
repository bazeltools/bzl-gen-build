use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use super::sha256_value::Sha256Value;
use crate::{async_read_json_file, async_write_json_file, write_json_file, Extract, Opt};
use anyhow::{anyhow, Context, Result};
use bzl_gen_build_shared_types::{
    api::extracted_data::ExtractedData, internal_types::tree_node::TreeNode,
    module_config::ModuleConfig, Directive, ProjectConf,
};
use ignore::WalkBuilder;
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

    let target_path = opt.sha_to_extract_root.join(format!("{}", sha256));

    let processed_file = ProcessedFile {
        file_path: path.clone(),
        sha256,
        extract_path: target_path.clone(),
    };

    if target_path.exists() {
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
        command.arg("--output").arg(target_path.as_path());
        command.kill_on_drop(true);
        let status = {
            let mut spawned_child = command.spawn()?;
            let status = spawned_child.wait().await?;
            status
        };

        if !status.success() {
            return Err(anyhow!("Failed to run program {:#?}", command));
        }
        if !target_path.exists() {
            return Err(anyhow!(
                "Ran sub process but the cache path doesn't exist still, expected it at {:?}",
                target_path
            ));
        }
        Ok((processed_file, st.elapsed()))
    }
}

async fn async_extract_def_refs(
    working_directory: &'static PathBuf,
    child_path: String,
    concurrent_io_operations: &'static Semaphore,
    opt: Arc<ExtractConfig>,
) -> Result<((PathBuf, Duration), Vec<ProcessedFile>)> {
    let mut results: Vec<
        tokio::task::JoinHandle<Result<(ProcessedFile, Duration), anyhow::Error>>,
    > = Vec::default();
    for entry in WalkBuilder::new(working_directory.join(child_path))
        .build()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().map(|e| e.is_file()).unwrap_or(false) {
            let extension = entry.path().extension();
            if let Some(ext) = extension {
                if opt.file_extensions.iter().any(|e| e.as_os_str() == ext) {
                    let opt = opt.clone();

                    let e = entry
                        .path()
                        .strip_prefix(working_directory.as_path())?
                        .to_path_buf();
                    results.push(tokio::spawn(async move {
                        process_file(
                            e,
                            working_directory,
                            entry.into_path(),
                            concurrent_io_operations,
                            opt,
                        )
                        .await
                    }));
                }
            }
        }
    }
    let mut max_duration = Duration::ZERO;
    let mut max_target = PathBuf::from("");
    let mut res2 = Vec::default();
    for r in results {
        let (e, dur) = r.await??;
        if dur > max_duration {
            max_duration = dur;
            max_target = e.file_path.clone();
        }
        res2.push(e);
    }
    Ok(((max_target, max_duration), res2))
}

async fn merge_defrefs(
    concurrent_io_operations: &Semaphore,
    path_sha_to_merged_defrefs: &'static Path,
    directory: String,
    project_conf: &'static ProjectConf,
    mut work_items: Vec<ProcessedFile>,
    sha_of_conf_config: Arc<String>,
) -> Result<(String, ExtractedMapping)> {
    work_items.sort_by(|a, b| a.sha256.cmp(&b.sha256));

    let merged_sha = Sha256Value::hash_iter_bytes(
        work_items
            .iter()
            .map(|e| e.sha256.as_bytes())
            .chain(std::iter::once(sha_of_conf_config.as_bytes())),
    );

    let treenode_path = path_sha_to_merged_defrefs.join(format!("{}.treenode", merged_sha));

    if !treenode_path.exists() {
        let mut existing: TreeNode = TreeNode::from_label(directory.clone());
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
            .filter(|directive| directory.starts_with(&directive.prefix))
            .flat_map(|e| e.directive_strings.iter())
            .cloned()
            .collect();

        let directives = Directive::from_strings(&directive_strings)?;
        existing.apply_directives(&directives);

        async_write_json_file(&treenode_path, &existing).await?;
        drop(c);
    };

    Ok((
        directory,
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
) -> Result<Vec<Arc<ExtractConfig>>> {
    let mut cfgs: Vec<Arc<ExtractConfig>> = Vec::default();
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

        cfgs.push(Arc::new(ExtractConfig {
            extractor,
            sha_to_extract_root: sha_to_extract_root.to_path_buf(),
            module_config: v,
            file_extensions: os_string_file_extensions,
        }));
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
        extract_path: path.clone(),
    };
    let work_items = vec![processed_file];
    merge_defrefs(
        concurrent_io_operations,
        path_sha_to_merged_defrefs,
        format!("sha256__{}", sha256),
        project_conf,
        work_items,
        sha_of_conf_config,
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
            results.push(tokio::spawn(async move {
                inner_load_external(
                    opt,
                    extract,
                    project_conf,
                    concurrent_io_operations,
                    path,
                    path_sha_to_merged_defrefs,
                    sha_of_conf_config,
                )
                .await
            }));
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
    let cfgs = extract_configs(opt, project_conf, sha_to_extract_root, extractors)?;

    let mut all_visiting_paths: Vec<(String, Arc<ExtractConfig>)> = Vec::default();
    for cfg in cfgs {
        all_visiting_paths.extend(
            cfg.module_config
                .main_roots
                .iter()
                .chain(cfg.module_config.test_roots.iter())
                .map(|e| (e.clone(), cfg.clone())),
        );
    }

    let mut async_join_handle: Vec<
        tokio::task::JoinHandle<Result<((PathBuf, Duration), Vec<ProcessedFile>)>>,
    > = Vec::default();
    for (path, extract_config) in all_visiting_paths.into_iter() {
        let extract_config = extract_config.clone();
        async_join_handle.push(tokio::spawn(async move {
            let r = async_extract_def_refs(
                &opt.working_directory,
                path,
                concurrent_io_operations,
                extract_config,
            )
            .await?;
            Ok(r)
        }));
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

    let extractors = load_extractors(extract).await?;

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

    let probe_files = tokio::spawn(async move {
        run_extractors_on_data(
            opt,
            project_conf,
            concurrent_io_operations,
            sha_to_extract_root,
            &extractors,
        )
        .await
    });

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

            let directory = if let Some(idx) = rel_path.rfind('/') {
                rel_path.split_at(idx).0.to_string()
            } else {
                rel_path
            };

            work.entry(directory).or_default().push(processed_file);
        }

        for (directory, files) in work.into_iter() {
            let sha_of_conf_config = sha_of_conf_config.clone();
            let path_sha_to_merged_defrefs = path_sha_to_merged_defrefs;
            merge_work.push(tokio::spawn(async move {
                merge_defrefs(
                    concurrent_io_operations,
                    path_sha_to_merged_defrefs,
                    directory,
                    project_conf,
                    files,
                    sha_of_conf_config,
                )
                .await
            }))
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
