mod manifest;
mod store;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use harmony_content::book::BookStore;

use crate::manifest::Manifest;
use crate::store::{cid_to_hex, FileBookStore};

#[derive(Parser)]
#[command(name = "cas-tool", about = "Content-addressed storage for asset pipeline")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Ingest files into the book store, producing a manifest
    Ingest {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Restore files from a manifest using the book store
    Restore {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        store: Option<PathBuf>,
    },
}

fn default_store_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(xdg).join("harmony-glitch/cas")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".cache/harmony-glitch/cas")
    } else {
        PathBuf::from(".cas")
    }
}

#[derive(Debug)]
struct IngestResult {
    total: usize,
    new: usize,
    unchanged: usize,
}

fn ingest(
    input_dir: &Path,
    manifest_path: &Path,
    store: &mut FileBookStore,
) -> Result<IngestResult, Box<dyn std::error::Error>> {
    // 1. Verify input_dir exists and is a directory
    if !input_dir.exists() || !input_dir.is_dir() {
        return Err(format!("input path is not a directory: {}", input_dir.display()).into());
    }

    // 2. Collect files (non-recursive, skip directories), sorted by name
    let mut files: Vec<_> = std::fs::read_dir(input_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .collect();
    files.sort_by_key(|e| e.file_name());

    // 3. Error if empty
    if files.is_empty() {
        return Err("input directory contains no files".into());
    }

    // 4. Load existing manifest (returns empty if file doesn't exist)
    let mut manifest = Manifest::load(manifest_path)?;

    let mut result = IngestResult {
        total: files.len(),
        new: 0,
        unchanged: 0,
    };

    // 5. For each file
    for entry in &files {
        let filename = entry.file_name().to_string_lossy().into_owned();

        // a. Read file data
        let data = std::fs::read(entry.path())?;

        // b. Insert into book store
        let cid = store.insert(&data)?;
        let cid_hex = cid_to_hex(&cid);

        // c. Compare CID hex with existing manifest entry
        let existing_hex = manifest.get_cid(&filename)?.map(|c| cid_to_hex(&c));

        // d. If same: unchanged++. If different or new: update manifest, new++
        if existing_hex.as_deref() == Some(cid_hex.as_str()) {
            result.unchanged += 1;
        } else {
            manifest.set(filename, &cid);
            result.new += 1;
        }
    }

    // 6. Save manifest
    manifest.save(manifest_path)?;

    // 7. Return IngestResult
    Ok(result)
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Ingest {
            input,
            manifest,
            store,
        } => {
            let store_dir = store.unwrap_or_else(default_store_dir);
            let mut book_store = FileBookStore::open(store_dir);
            let result = ingest(&input, &manifest, &mut book_store).expect("ingest failed");
            println!(
                "Ingested {} files ({} new, {} unchanged)",
                result.total, result.new, result.unchanged
            );
        }
        Command::Restore { .. } => {
            eprintln!("restore not yet implemented");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harmony_content::cid::ContentId;

    fn write_file(dir: &Path, name: &str, data: &[u8]) {
        std::fs::write(dir.join(name), data).unwrap();
    }

    fn make_store(dir: &Path) -> FileBookStore {
        FileBookStore::open(dir.join("store"))
    }

    #[test]
    fn ingest_produces_correct_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("input");
        std::fs::create_dir_all(&input).unwrap();

        let data_a = b"content of file a";
        let data_b = b"content of file b";
        write_file(&input, "a.txt", data_a);
        write_file(&input, "b.txt", data_b);

        let manifest_path = tmp.path().join("manifest.json");
        let mut store = make_store(tmp.path());

        let result = ingest(&input, &manifest_path, &mut store).unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.new, 2);
        assert_eq!(result.unchanged, 0);

        let manifest = Manifest::load(&manifest_path).unwrap();
        let cid_a = manifest.get_cid("a.txt").unwrap().unwrap();
        let cid_b = manifest.get_cid("b.txt").unwrap().unwrap();

        assert!(
            cid_a.verify_hash(data_a),
            "cid_a should verify against data_a"
        );
        assert!(
            cid_b.verify_hash(data_b),
            "cid_b should verify against data_b"
        );
    }

    #[test]
    fn ingest_idempotent_on_unchanged_files() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("input");
        std::fs::create_dir_all(&input).unwrap();

        write_file(&input, "file.txt", b"stable content");

        let manifest_path = tmp.path().join("manifest.json");
        let mut store = make_store(tmp.path());

        // First ingest
        let r1 = ingest(&input, &manifest_path, &mut store).unwrap();
        assert_eq!(r1.new, 1);
        assert_eq!(r1.unchanged, 0);

        // Second ingest with same data
        let r2 = ingest(&input, &manifest_path, &mut store).unwrap();
        assert_eq!(r2.new, 0);
        assert_eq!(r2.unchanged, 1);
    }

    #[test]
    fn ingest_detects_changed_file() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("input");
        std::fs::create_dir_all(&input).unwrap();

        let data_v1 = b"version one content";
        let data_v2 = b"version two content";
        write_file(&input, "file.txt", data_v1);

        let manifest_path = tmp.path().join("manifest.json");
        let mut store = make_store(tmp.path());

        // Ingest v1
        let r1 = ingest(&input, &manifest_path, &mut store).unwrap();
        assert_eq!(r1.new, 1);

        let manifest_v1 = Manifest::load(&manifest_path).unwrap();
        let cid_v1 = manifest_v1.get_cid("file.txt").unwrap().unwrap();
        assert!(cid_v1.verify_hash(data_v1));

        // Change file and ingest again
        write_file(&input, "file.txt", data_v2);
        let r2 = ingest(&input, &manifest_path, &mut store).unwrap();
        assert_eq!(r2.new, 1);
        assert_eq!(r2.unchanged, 0);

        let manifest_v2 = Manifest::load(&manifest_path).unwrap();
        let cid_v2 = manifest_v2.get_cid("file.txt").unwrap().unwrap();
        assert!(
            cid_v2.verify_hash(data_v2),
            "manifest should have updated CID matching v2 data"
        );
        assert_ne!(
            ContentId::for_book(data_v1, harmony_content::cid::ContentFlags::default()).unwrap(),
            cid_v2,
            "v2 CID should differ from v1"
        );
    }

    #[test]
    fn ingest_empty_directory_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("empty");
        std::fs::create_dir_all(&input).unwrap();

        let manifest_path = tmp.path().join("manifest.json");
        let mut store = make_store(tmp.path());

        let result = ingest(&input, &manifest_path, &mut store);
        assert!(result.is_err(), "ingest of empty dir should return Err");
    }
}
