use clap::Parser;
use serde_json;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

use bzl_gen_jarscanner as jarscanner;

#[derive(Parser)]
#[command(author, version, about)]
#[command(version = "0.1.0")]
#[command(about = "Jar to containing classes tool", long_about = None)]
struct Opt {
    #[arg(long)]
    out: PathBuf,

    #[arg(long)]
    label: String,

    #[arg(long)]
    input_jar: PathBuf,

    #[arg(long)]
    relative_path: String,

    #[arg(long)]
    label_to_allowed_prefixes: Option<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let opt = Opt::parse();

    let label_to_allowed_prefixes: HashMap<String, Vec<String>> =
        match opt.label_to_allowed_prefixes {
            Some(lp) => serde_json::from_str(&lp)?,
            None => HashMap::new(),
        };

    let target_descriptor = jarscanner::process_input(
        &opt.label,
        &opt.input_jar,
        &opt.relative_path,
        &label_to_allowed_prefixes,
    )?;
    jarscanner::emit_result(&target_descriptor, &opt.out)?;
    Ok(())
}
