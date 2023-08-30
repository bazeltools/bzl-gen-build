use anyhow::{Context, Result};
use bzl_gen_build_shared_types::api::extracted_data::{DataBlock, ExtractedData};
use clap::Parser;
use log::debug;
use std::{collections::{HashSet, BTreeSet}, path::PathBuf, time::Instant};

mod extract_protobuf_imports;
use extract_protobuf_imports::ProtobufSource;

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
        builder.parse_filters("warn,protobuf_extractor=info,bzl_gen_build_shared_types=info");
    }
    builder.init();

    let start_time = Instant::now();

    let mut relative_input_paths: Vec<String> =
        if let Some(suffix) = opt.relative_input_paths.strip_prefix('@') {
            std::fs::read_to_string(PathBuf::from(suffix))?
                .lines()
                .map(|e| e.to_string())
                .collect()
        } else {
            vec![opt.relative_input_paths.clone()]
        };

    relative_input_paths.sort();

    let mut data_blocks: Vec<DataBlock> = Default::default();

    for relative_path in relative_input_paths {
        let input_file = opt.working_directory.join(&relative_path);
        let mut refs: HashSet<String> = Default::default();
        let mut defs: BTreeSet<String> = Default::default();
        let mut bzl_gen_build_commands: HashSet<String> = Default::default();

        let input_str = std::fs::read_to_string(&input_file).with_context(|| {
            format!(
                "While attempting to load up file: {:?
    }",
                input_file
            )
        })?;

        let program = ProtobufSource::parse(&input_str, &relative_path).with_context(|| {
            format!(
                "Error while parsing file {:?
    }",
                input_file
            )
        })?;

        if !program.bzl_gen_build_commands.is_empty() {
            bzl_gen_build_commands.extend(program.bzl_gen_build_commands);
        }
        if !opt.disable_ref_generation {
            refs.extend(program.imports);
        }
        if !program.well_known_refs.is_empty() {
            bzl_gen_build_commands.extend(
                program
                    .well_known_refs
                    .into_iter()
                    .map(|x| format!("manual_ref:{}", x)),
            )
        }

        // See https://protobuf.dev/programming-guides/proto3/#importing
        // Protobuf uses file path relative to the workspace as a way of importing:
        //     import "myproject/other_protos.proto";
        // So here we export the relative path as a "definition".
        defs.extend(Some(relative_path.clone()));

        data_blocks.push(DataBlock {
            entity_path: relative_path,
            defs,
            refs,
            bzl_gen_build_commands,
        })
    }

    let def_refs = ExtractedData {
        label_or_repo_path: opt.label_or_repo_path.clone(),
        data_blocks,
    };

    tokio::fs::write(opt.output, serde_json::to_string_pretty(&def_refs)?).await?;

    debug!("took {:?}", start_time.elapsed());

    Ok(())
}
