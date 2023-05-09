use clap::Parser;
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
    out: String,

    #[arg(long)]
    label: String,

    #[arg(long)]
    input_jar: String,

    #[arg(long)]
    relative_path: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opt = Opt::parse();
    let output_path = PathBuf::from(opt.out);
    let input_jar_path = PathBuf::from(opt.input_jar);

    let mut label_to_allowed_prefixes = HashMap::new();
    label_to_allowed_prefixes.insert(
        "@jvm__com_netflix_iceberg__bdp_iceberg_spark_2_11//:jar".to_string(),
        vec!["com.netflix.iceberg.".to_string()],
    );

    let target_descriptor = jarscanner::process_input(
        &opt.label,
        &input_jar_path,
        &opt.relative_path,
        &label_to_allowed_prefixes,
    )?;
    jarscanner::emit_result(&target_descriptor, &output_path)?;
    Ok(())
}
