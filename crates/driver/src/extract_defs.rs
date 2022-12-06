use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use super::sha256_value::Sha256Value;
use crate::{
    async_read_json_file, async_write_json_file, extract_defrefs::ExtractedMapping, read_json_file,
    write_json_file, ExtractDefs, Opt,
};
use anyhow::{anyhow, Context, Result};
use bzl_gen_build_shared_types::{internal_types::tree_node::TreeNode, *};
use futures::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use super::extract_defrefs::ExtractedMappings;

#[derive(Debug, Serialize, Deserialize)]
pub struct PathToDefs {
    pub relative_path_to_defs: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DefsData {
    pub defs: Vec<String>,
}

async fn async_extract_defs(
    concurrent_io_operations: &Semaphore,
    path_sha_to_exports: Arc<PathBuf>,
    directory: String,
    mut work_items: Vec<ExtractedMapping>,
) -> Result<(String, String)> {
    work_items.sort_by(|a, b| a.content_sha.cmp(&b.content_sha));
    let merged_sha =
        Sha256Value::hash_iter_bytes(work_items.iter().map(|e| e.content_sha.as_bytes()));

    let target_path = path_sha_to_exports.join(format!("{}", merged_sha));

    if !target_path.exists() {
        let c = concurrent_io_operations.acquire().await?;
        let mut tree_nodes: HashSet<String> = HashSet::default();
        for ele in work_items.iter() {
            let d: TreeNode = async_read_json_file(PathBuf::from(&ele.path).as_path())
                .await
                .with_context(|| format!("Was attempting to read file data: {:#?}", ele))?;
            tree_nodes.extend(d.defs);
        }
        drop(c);

        let dd = DefsData {
            defs: tree_nodes.into_iter().collect(),
        };
        async_write_json_file(&target_path, dd).await?;
    }
    Ok((directory, target_path.to_string_lossy().to_string()))
}

pub async fn extract_exports(
    opt: &'static Opt,
    extract: &'static ExtractDefs,
    _project_conf: &ProjectConf,
    concurrent_io_operations: &'static Semaphore,
) -> Result<()> {
    let path_sha_to_exports = opt.cache_path.join("path_sha_to_exports");
    if !path_sha_to_exports.exists() {
        std::fs::create_dir_all(&path_sha_to_exports)?;
    }

    let path_sha_to_exports = Arc::new(path_sha_to_exports);

    let extracted_mappings: ExtractedMappings = read_json_file(&extract.extracted_mappings)?;

    let mut work: HashMap<String, Vec<ExtractedMapping>> = HashMap::default();
    for (k, content_path) in extracted_mappings.relative_path_to_extractmapping.iter() {
        let directory = if let Some(idx) = k.rfind('/') {
            let u_k = k.split_at(idx).0;

            u_k.rfind('/').map(|e| u_k.split_at(e).0).unwrap_or(u_k)
        } else {
            k.as_str()
        };
        if let Some(v) = work.get_mut(directory) {
            v.push(content_path.clone());
        } else {
            work.insert(directory.to_string(), vec![content_path.clone()]);
        }
    }

    let mut visited_paths = stream::iter(work.into_iter()).map(|(directory, work_items)| {
        let path_sha_to_exports = path_sha_to_exports.clone();
        tokio::spawn(async {
            async_extract_defs(
                concurrent_io_operations,
                path_sha_to_exports,
                directory,
                work_items,
            )
            .await
        })
    });

    let mut result: HashMap<String, String> = HashMap::default();

    while let Some(r) = visited_paths.next().await {
        let inner_r = r.await.map_err(|e| anyhow!("{:#?}", e))?;

        match inner_r {
            Ok((k, v)) => {
                result.insert(k, v);
            }
            Err(e) => return Err(e),
        }
    }

    let r = PathToDefs {
        relative_path_to_defs: result,
    };

    write_json_file(extract.extracted_defs.as_path(), &r)?;

    Ok(())
}
