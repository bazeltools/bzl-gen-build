pub mod build_graph;
pub mod extract_defrefs;
pub mod extract_defs;
pub mod print_build;
pub mod sha256_value;

use std::{
    borrow::Cow,
    collections::HashSet,
    io::Read,
    path::{Path, PathBuf},
    time::Instant,
};

use bzl_gen_build_shared_types::*;

use clap::{Args, Parser, Subcommand};

use anyhow::{Context, Result};

use log::info;
use tokio::{io::AsyncReadExt, sync::Semaphore};

#[derive(Debug, Subcommand)]
pub enum Commands {
    Extract(Extract),
    ExtractDefs(ExtractDefs),
    BuildGraph(BuildGraphArgs),
    PrintBuild(PrintBuildArgs),
}

#[derive(Debug, Args)]
pub struct Extract {
    #[clap(long)]
    // A set of named_group:extractorpath
    extractor: Vec<String>,

    #[clap(long)]
    external_generated_root: Option<PathBuf>,

    #[clap(long)]
    extracted_mappings: PathBuf,
}

#[derive(Debug, Args)]
pub struct ExtractDefs {
    #[clap(long)]
    extracted_mappings: PathBuf,

    #[clap(long)]
    extracted_defs: PathBuf,
}

#[derive(Debug, Args)]
pub struct BuildGraphArgs {
    #[clap(long)]
    extracted_mappings: PathBuf,

    #[clap(long)]
    extracted_defs: PathBuf,

    #[clap(long)]
    external_label_to_defref: Option<PathBuf>,

    #[clap(long)]
    graph_out: PathBuf,
}

#[derive(Debug, Args)]
pub struct PrintBuildArgs {
    #[clap(long)]
    graph_data: PathBuf,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Opt {
    #[clap(long)]
    input_path: PathBuf,

    #[clap(long)]
    working_directory: PathBuf,

    #[clap(long, default_value_t = 8)]
    concurrent_io_operations: usize,

    #[clap(long)]
    cache_path: PathBuf,

    /// generate one target per source file, instead of aggregating *.proto etc.
    #[clap(long)]
    no_aggregate_source: bool,

    #[command(subcommand)]
    command: Commands,
}

pub fn read_json_file<T>(p: &Path) -> Result<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let mut file = std::fs::File::open(p).with_context(|| format!("Opening json file {:?}", p))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let v: T = serde_json::from_str(contents.as_str())?;

    drop(contents);
    Ok(v)
}

pub async fn async_read_json_file<T>(p: &Path) -> Result<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let mut file = tokio::fs::File::open(p)
        .await
        .with_context(|| format!("Opening json file {:?}", p))?;
    let file_len: u64 = file.metadata().await?.len();
    let mut contents = String::with_capacity(file_len as usize);
    file.read_to_string(&mut contents).await?;

    let v: T = serde_json::from_str(contents.as_str())?;

    drop(contents);
    Ok(v)
}

pub fn write_json_file<T>(p: &Path, value: T) -> Result<()>
where
    T: serde::Serialize,
{
    std::fs::write(p, serde_json::to_string_pretty(&value)?)?;
    Ok(())
}

pub async fn async_write_json_file<T>(p: &Path, value: T) -> Result<()>
where
    T: serde::Serialize,
{
    tokio::fs::write(p, serde_json::to_string_pretty(&value)?).await?;
    Ok(())
}

fn maybe_add_working_directory<'a, 'b>(
    working_directory: &'a Path,
    path: &'b Path,
) -> Cow<'b, Path> {
    if path.is_absolute() {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(working_directory.join(path))
    }
}

fn read_all_project_conf(input_path: &Path, working_directory: &Path) -> Result<ProjectConf> {
    let mut v: bzl_gen_build_shared_types::ProjectConf =
        read_json_file(maybe_add_working_directory(working_directory, input_path).as_ref())
            .with_context(|| "Reading main config file")?;

    let mut seen_includes: HashSet<String> = HashSet::default();
    while !v.includes.is_empty() {
        let mut includes = std::mem::take(&mut v.includes);
        while let Some(p) = includes.pop() {
            if seen_includes.contains(&p) {
                continue;
            }
            seen_includes.insert(p.clone());
            let include_path = PathBuf::from(p);
            let path = if include_path.is_relative() {
                working_directory.join(include_path)
            } else {
                include_path
            };
            let mut nxt: bzl_gen_build_shared_types::ProjectConf =
                read_json_file(path.as_path())
                    .with_context(|| format!("Reading input config {}", path.display()))?;
            for e in std::mem::take(&mut nxt.includes).into_iter() {
                includes.push(e);
            }
            v.merge(nxt);
        }
    }

    Ok(v)
}

pub fn to_directory(rel_path: String) -> String {
    if let Some(idx) = rel_path.rfind('/') {
        rel_path.split_at(idx).0.to_string()
    } else {
        rel_path
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let opt = Opt::parse();

    let mut builder = pretty_env_logger::formatted_timed_builder();
    builder.format_timestamp_nanos();
    builder.target(pretty_env_logger::env_logger::Target::Stderr);
    if let Ok(s) = ::std::env::var("RUST_LOG") {
        builder.parse_filters(&s);
    } else {
        builder.parse_filters("warn,bzl_gen_build_driver=info,bzl_gen_build_shared_types=info");
    }
    builder.init();

    let concurrent_io_operations =
        Box::leak(Box::new(Semaphore::new(opt.concurrent_io_operations)));
    let opt = Box::leak(Box::new(opt));

    if !opt.cache_path.exists() {
        std::fs::create_dir_all(&opt.cache_path)?;
    }

    let v = Box::leak(Box::new(read_all_project_conf(
        opt.input_path.as_path(),
        opt.working_directory.as_path(),
    )?));

    let start_time = Instant::now();
    match &opt.command {
        Commands::Extract(e) => {
            extract_defrefs::extract_defrefs(opt, &e, v, concurrent_io_operations).await?
        }
        Commands::ExtractDefs(e) => {
            extract_defs::extract_exports(opt, &e, v, concurrent_io_operations).await?
        }
        Commands::BuildGraph(e) => {
            build_graph::build_graph(opt, &e, v, concurrent_io_operations).await?
        }
        Commands::PrintBuild(e) => {
            print_build::print_build(opt, &e, v, concurrent_io_operations).await?
        }
    };

    let all_processed = start_time.elapsed();

    info!("Command {:?} took {:?}", opt.command, all_processed);
    Ok(())
}
