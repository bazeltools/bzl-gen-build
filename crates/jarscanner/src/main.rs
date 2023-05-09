use clap::Parser;
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


fn main() {
    let opt = Opt::parse();
    let output_path = PathBuf::from(opt.out);
    let input_jar_path = PathBuf::from(opt.input_jar);

}
