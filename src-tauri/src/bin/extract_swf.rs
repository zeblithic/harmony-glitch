use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "extract-swf", about = "Extract bitmaps from SWF files")]
struct Args {
    /// Source directory containing SWF files
    #[arg(long)]
    source: PathBuf,

    /// Output directory for extracted PNGs
    #[arg(long)]
    output: PathBuf,
}

fn main() {
    let args = Args::parse();

    if !args.source.is_dir() {
        eprintln!("Error: source directory does not exist: {}", args.source.display());
        std::process::exit(1);
    }

    std::fs::create_dir_all(&args.output).unwrap_or_else(|e| {
        eprintln!("Error: cannot create output directory: {e}");
        std::process::exit(1);
    });

    println!("Source: {}", args.source.display());
    println!("Output: {}", args.output.display());
    println!("(extraction not yet implemented)");
}
