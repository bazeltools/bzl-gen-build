use anyhow::Result;
use clap::Parser;
use log::debug;
use std::path::PathBuf;
use std::time::Instant;

use bzl_gen_python_extractor as pe;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Opt {
    #[clap(long)]
    /// comma sepearted list of input files
    relative_input_paths: String,

    #[clap(long)]
    /// comma sepearted list of input files
    working_directory: PathBuf,

    #[clap(long)]
    output: PathBuf,

    #[clap(long)]
    label_or_repo_path: String,

    #[clap(long)]
    disable_ref_generation: bool,

    /// When specified we calculate refs relative to here rather than using a heuristic
    #[clap(long)]
    import_path_relative_from: Option<String>,
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
        builder.parse_filters("warn,python_extractor=info,bzl_gen_build_shared_types=info");
    }
    builder.init();

    let start_time = Instant::now();

    pe::extract_python(
        opt.relative_input_paths,
        opt.working_directory,
        opt.output,
        opt.label_or_repo_path,
        opt.disable_ref_generation,
        opt.import_path_relative_from,
    )
    .await?;

    debug!("took {:?}", start_time.elapsed());
    Ok(())
}
