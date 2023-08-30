use clap::Parser;
use serde_json;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

use bzl_gen_jarscanner as jarscanner;
use bzl_gen_jarscanner::errors::LabelToAllowedPrefixesError;

#[derive(Parser)]
#[command(author, version, about)]
#[command(version = "0.1.0")]
#[command(about = "Jar to containing classes tool", long_about = None)]
struct Opt {
    #[arg(long, help = "Path to json output file")]
    out: PathBuf,

    #[arg(long, help = "Input bazel label")]
    label: String,

    #[arg(long, help = "Path to jar file to scan")]
    input_jar: PathBuf,

    #[arg(long, help = "Relative path to the file inside its tree")]
    relative_path: String,

    #[arg(
        long,
        help = "JSON HashMap<String, Vec<String>>. This is a setting such that we can say some jars are only allowed to produce some class prefixes. This mostly comes up only when someone has published a form of fat jar (Iceberg and zeppelin do this). Anything not in this map has no filter guard."
    )]
    label_to_allowed_prefixes: Option<String>,
}

fn main() -> Result<(), Box<dyn Error>> {

    let mut idx = 0;
    while idx < 1000 {

        let opt = Opt::parse();

        let label_to_allowed_prefixes: HashMap<String, Vec<String>> =
            match opt.label_to_allowed_prefixes {
                Some(lp) => serde_json::from_str(&lp).map_err(|e| LabelToAllowedPrefixesError {
                    json_deser_error: e.to_string(),
                })?,
                None => HashMap::new(),
            };

        let target_descriptor = jarscanner::process_input(
            &opt.label,
            &opt.input_jar,
            &opt.relative_path,
            &label_to_allowed_prefixes,
        )?;
        jarscanner::emit_result(&target_descriptor, &opt.out)?;
        idx += 1;

    }
    Ok(())
}
