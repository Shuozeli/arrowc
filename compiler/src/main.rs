use std::path::PathBuf;
use std::process;

use clap::Parser;

#[derive(Parser)]
#[command(name = "arrowc", about = "Arrow schema compiler")]
struct Cli {
    /// Input schema files (JSON or YAML).
    #[arg(required = true)]
    files: Vec<PathBuf>,

    /// Output directory for generated files.
    #[arg(short, long, default_value = ".")]
    output: PathBuf,

    /// Input format (auto-detected from extension if omitted).
    #[arg(long, value_parser = parse_format)]
    format: Option<arrowc::InputFormat>,

    /// Generate Rust code (default).
    #[arg(long, default_value_t = true)]
    rust: bool,
}

fn parse_format(s: &str) -> Result<arrowc::InputFormat, String> {
    match s {
        "json" => Ok(arrowc::InputFormat::Json),
        "yaml" | "yml" => Ok(arrowc::InputFormat::Yaml),
        _ => Err(format!("unknown format: {s} (expected json or yaml)")),
    }
}

fn main() {
    let cli = Cli::parse();

    let mut had_error = false;

    for file in &cli.files {
        match arrowc::compile(file, &cli.output, cli.format) {
            Ok(written) => {
                for path in &written {
                    eprintln!("wrote {path}");
                }
            }
            Err(e) => {
                eprintln!("error: {}: {e}", file.display());
                had_error = true;
            }
        }
    }

    if had_error {
        process::exit(1);
    }
}
