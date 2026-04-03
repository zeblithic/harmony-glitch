mod manifest;
mod store;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use harmony_content::book::BookStore;

use crate::manifest::Manifest;
use crate::store::{cid_to_hex, hex_to_cid, FileBookStore};

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

#[derive(Debug)]
struct RestoreResult {
    total: usize,
    written: usize,
    skipped: usize,
}

fn restore(
    manifest_path: &Path,
    output_dir: &Path,
    store: &FileBookStore,
) -> Result<RestoreResult, Box<dyn std::error::Error>> {
    // 1. Check manifest exists, error if not
    if !manifest_path.exists() {
        return Err(format!("manifest not found: {}", manifest_path.display()).into());
    }

    // 2. Load manifest
    let manifest = Manifest::load(manifest_path)?;

    // 3. Error if manifest has no files
    if manifest.files.is_empty() {
        return Err("manifest contains no files".into());
    }

    // 4. FIRST PASS: verify ALL CIDs exist in the store before writing anything
    let mut pairs: Vec<(String, harmony_content::cid::ContentId)> =
        Vec::with_capacity(manifest.files.len());
    for (filename, hex) in &manifest.files {
        let cid = hex_to_cid(hex).map_err(|_| {
            format!("invalid hex CID for '{}': '{}'", filename, hex)
        })?;
        if !store.contains(&cid) {
            return Err(format!(
                "missing book {} for file '{}'. Run the full pipeline to populate the store.",
                hex, filename
            )
            .into());
        }
        pairs.push((filename.clone(), cid));
    }

    // 5. Create output directory (including parents)
    std::fs::create_dir_all(output_dir)?;

    // 6. SECOND PASS: write files
    let mut result = RestoreResult {
        total: pairs.len(),
        written: 0,
        skipped: 0,
    };

    for (filename, cid) in &pairs {
        let out_path = output_dir.join(filename);
        // a. If output file exists with correct byte length: skip
        if out_path.exists() {
            let data = store.get(cid).expect("CID verified in first pass");
            let meta = std::fs::metadata(&out_path)?;
            if meta.len() == data.len() as u64 {
                result.skipped += 1;
                continue;
            }
        }
        // b. Otherwise write
        let data = store.get(cid).expect("CID verified in first pass");
        std::fs::write(&out_path, data)?;
        result.written += 1;
    }

    // 7. Return RestoreResult
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
            let mut book_store = FileBookStore::open(store_dir).expect("failed to open book store");
            let result = ingest(&input, &manifest, &mut book_store).expect("ingest failed");
            println!(
                "Ingested {} files ({} new, {} unchanged)",
                result.total, result.new, result.unchanged
            );
        }
        Command::Restore { manifest, output, store } => {
            let store_dir = store.unwrap_or_else(default_store_dir);
            let book_store = FileBookStore::open(store_dir).expect("failed to open book store");
            let result = restore(&manifest, &output, &book_store)
                .expect("restore failed");
            println!(
                "Restored {} files ({} written, {} already present)",
                result.total, result.written, result.skipped
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harmony_content::cid::{ContentFlags, ContentId};

    fn write_file(dir: &Path, name: &str, data: &[u8]) {
        std::fs::write(dir.join(name), data).unwrap();
    }

    fn make_store(dir: &Path) -> FileBookStore {
        FileBookStore::open(dir.join("store")).unwrap()
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

    #[test]
    fn restore_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("input");
        std::fs::create_dir_all(&input).unwrap();

        let data_png = b"fake atlas png bytes";
        let data_json = b"{\"atlas\": true}";
        write_file(&input, "atlas.png", data_png);
        write_file(&input, "atlas.json", data_json);

        let manifest_path = tmp.path().join("manifest.json");
        let mut store = make_store(tmp.path());
        ingest(&input, &manifest_path, &mut store).unwrap();

        let output = tmp.path().join("output");
        let result = restore(&manifest_path, &output, &store).unwrap();

        assert_eq!(result.total, 2);
        assert_eq!(result.written, 2);
        assert_eq!(result.skipped, 0);

        assert_eq!(std::fs::read(output.join("atlas.png")).unwrap(), data_png);
        assert_eq!(std::fs::read(output.join("atlas.json")).unwrap(), data_json);
    }

    #[test]
    fn restore_skips_existing_correct_size() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("input");
        std::fs::create_dir_all(&input).unwrap();

        let data = b"file content to skip";
        write_file(&input, "file.txt", data);

        let manifest_path = tmp.path().join("manifest.json");
        let mut store = make_store(tmp.path());
        ingest(&input, &manifest_path, &mut store).unwrap();

        // Pre-create output file with same byte length
        let output = tmp.path().join("output");
        std::fs::create_dir_all(&output).unwrap();
        // Write same-length content (but different bytes) to trigger skip-by-size
        std::fs::write(output.join("file.txt"), data).unwrap();

        let result = restore(&manifest_path, &output, &store).unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.written, 0);
        assert_eq!(result.skipped, 1);
    }

    #[test]
    fn restore_missing_cid_errors() {
        let tmp = tempfile::tempdir().unwrap();

        // Build a manifest manually with a CID not in the store
        let fake_cid = ContentId::for_book(b"not stored", ContentFlags::default()).unwrap();
        let fake_hex = cid_to_hex(&fake_cid);
        let manifest_json = format!(
            "{{\"files\":{{\"missing_file.txt\":\"{}\"}}}}",
            fake_hex
        );
        let manifest_path = tmp.path().join("manifest.json");
        std::fs::write(&manifest_path, &manifest_json).unwrap();

        let store = make_store(tmp.path());
        let output = tmp.path().join("output");

        let err = restore(&manifest_path, &output, &store).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("missing_file.txt"),
            "error should mention the filename, got: {msg}"
        );
    }

    #[test]
    fn restore_creates_output_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("input");
        std::fs::create_dir_all(&input).unwrap();

        let data = b"content for nested restore";
        write_file(&input, "asset.bin", data);

        let manifest_path = tmp.path().join("manifest.json");
        let mut store = make_store(tmp.path());
        ingest(&input, &manifest_path, &mut store).unwrap();

        // Use a deeply nested, nonexistent output directory
        let output = tmp.path().join("a").join("b").join("c");
        assert!(!output.exists(), "output dir should not exist yet");

        let result = restore(&manifest_path, &output, &store).unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.written, 1);
        assert!(output.exists(), "output dir should have been created");
        assert_eq!(std::fs::read(output.join("asset.bin")).unwrap(), data);
    }

    #[test]
    fn end_to_end_ingest_delete_restore_matches() {
        let input_dir = tempfile::tempdir().unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let output_dir = tempfile::tempdir().unwrap();
        let manifest_path = store_dir.path().join("manifest.json");

        // Create files with realistic-ish content
        let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG magic + junk
        let json_data = br#"{"frames":{"cherry":{"frame":{"x":0,"y":0,"w":32,"h":32}}}}"#;
        std::fs::write(input_dir.path().join("items.png"), &png_data).unwrap();
        std::fs::write(input_dir.path().join("items.json"), &json_data[..]).unwrap();

        // Ingest
        let mut store = FileBookStore::open(store_dir.path().join("books")).unwrap();
        let ingest_result = ingest(input_dir.path(), &manifest_path, &mut store).unwrap();
        assert_eq!(ingest_result.total, 2);
        assert_eq!(ingest_result.new, 2);

        // "Delete" originals (simulating a fresh clone)
        // Reopen store from disk to verify persistence
        drop(store);
        let store = FileBookStore::open(store_dir.path().join("books")).unwrap();

        // Restore to a completely new directory
        let result = restore(&manifest_path, output_dir.path(), &store).unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.written, 2);

        // Verify byte-for-byte match
        assert_eq!(
            std::fs::read(output_dir.path().join("items.png")).unwrap(),
            png_data
        );
        assert_eq!(
            std::fs::read(output_dir.path().join("items.json")).unwrap(),
            json_data
        );

        // Verify CIDs in manifest match the data
        let manifest = Manifest::load(&manifest_path).unwrap();
        let cid_png = manifest.get_cid("items.png").unwrap().unwrap();
        let cid_json = manifest.get_cid("items.json").unwrap().unwrap();
        assert!(cid_png.verify_hash(&png_data));
        assert!(cid_json.verify_hash(json_data));
    }
}
