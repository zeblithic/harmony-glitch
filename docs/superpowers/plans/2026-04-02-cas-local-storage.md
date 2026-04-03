# CAS Local Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate harmony-content's content-addressed storage into the asset pipeline so large atlas blobs live in a local book store instead of git, with manifests tracking content IDs.

**Architecture:** A new `cas-tool` Rust binary with `ingest` and `restore` subcommands, backed by a `FileBookStore` that stores books as files in the XDG cache directory. The pipeline chains `extract → pack → cas ingest`, and `cas restore` materializes files from manifests.

**Tech Stack:** Rust, harmony-content (ContentId, BookStore trait), clap (CLI), serde_json (manifests), hex (CID encoding)

---

**Spec:** `docs/superpowers/specs/2026-04-02-cas-local-storage-design.md`

## File Structure

```
src-tauri/src/bin/cas_tool/
  main.rs       — CLI entry point with clap subcommands (ingest, restore)
  store.rs      — FileBookStore: disk-backed BookStore implementation
  manifest.rs   — Manifest type: load, save, filename→CID mapping

manifests/
  (populated by running cas-tool ingest, committed to git)

src-tauri/Cargo.toml  — new [[bin]] target, harmony-content dep, cas-tool feature
package.json          — new npm scripts (ingest-items, restore-items)
.gitignore            — add assets/sprites/ exclusion
```

## Context for Implementers

### harmony-content API

The `BookStore` trait lives in `harmony_content::book`:

```rust
pub trait BookStore {
    fn insert_with_flags(&mut self, data: &[u8], flags: ContentFlags) -> Result<ContentId, ContentError>;
    fn insert(&mut self, data: &[u8]) -> Result<ContentId, ContentError>; // default: flags=default()
    fn store(&mut self, cid: ContentId, data: Vec<u8>);
    fn get(&self, cid: &ContentId) -> Option<&[u8]>;
    fn contains(&self, cid: &ContentId) -> bool;
    fn remove(&mut self, cid: &ContentId) -> Option<Vec<u8>>;
}
```

`ContentId` is a 32-byte value (`[u8; 4]` header + `[u8; 28]` hash). Key methods:
- `ContentId::for_book(data, flags) -> Result<ContentId>` — compute CID for data
- `cid.to_bytes() -> [u8; 32]` — serialize to bytes
- `ContentId::from_bytes([u8; 32]) -> ContentId` — deserialize
- `cid.verify_hash(data) -> bool` — check data matches CID

For manifests, we encode CIDs as 64-char lowercase hex strings using `hex::encode(cid.to_bytes())` and decode with `hex::decode(s)`.

### Existing Pattern: extract-swf binary

The `extract-swf` binary at `src-tauri/src/bin/extract_swf/main.rs` shows the pattern:
- `[[bin]]` in Cargo.toml with `required-features`
- `mod` declarations for sibling files
- `clap::Parser` for CLI args
- Feature-gated optional deps

### XDG Cache Path

Default store: `$XDG_CACHE_HOME/harmony-glitch/cas/` → fallback `~/.cache/harmony-glitch/cas/`.

```rust
fn default_store_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(xdg).join("harmony-glitch/cas")
    } else if let Some(home) = std::env::var("HOME").ok() {
        PathBuf::from(home).join(".cache/harmony-glitch/cas")
    } else {
        // Fallback for weird environments
        PathBuf::from(".cas")
    }
}
```

---

### Task 1: Cargo.toml — Add harmony-content dependency and cas-tool binary target

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add harmony-content dependency and cas-tool feature**

In `src-tauri/Cargo.toml`, add the dependency and feature:

```toml
# In [dependencies], after the harmony-zenoh line:
harmony-content = { path = "../../harmony/crates/harmony-content", optional = true }

# In [features], add:
cas-tool = ["clap", "dep:harmony-content"]
```

And add the `[[bin]]` target after the existing extract-swf one:

```toml
[[bin]]
name = "cas-tool"
path = "src/bin/cas_tool/main.rs"
required-features = ["cas-tool"]
```

- [ ] **Step 2: Verify it compiles**

Run:
```bash
cd src-tauri && cargo check --features cas-tool
```

Expected: Compilation error about missing `src/bin/cas_tool/main.rs` — that's fine, we'll create it next.

- [ ] **Step 3: Create minimal main.rs stub**

Create `src-tauri/src/bin/cas_tool/main.rs`:

```rust
fn main() {
    println!("cas-tool stub");
}
```

- [ ] **Step 4: Verify it compiles and runs**

Run:
```bash
cd src-tauri && cargo run --features cas-tool --bin cas-tool
```

Expected: Prints `cas-tool stub`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/bin/cas_tool/main.rs
git commit -m "feat: add cas-tool binary target with harmony-content dependency"
```

---

### Task 2: FileBookStore — Disk-backed BookStore implementation

**Files:**
- Create: `src-tauri/src/bin/cas_tool/store.rs`
- Modify: `src-tauri/src/bin/cas_tool/main.rs` (add `mod store;`)

- [ ] **Step 1: Write failing tests for FileBookStore**

Create `src-tauri/src/bin/cas_tool/store.rs` with tests only (no implementation):

```rust
use std::path::PathBuf;

use harmony_content::book::BookStore;
use harmony_content::cid::{ContentFlags, ContentId};

/// Disk-backed BookStore that stores books as files in a directory.
///
/// On open, eagerly loads all existing .book files into an in-memory cache.
/// New inserts write to both cache and disk.
pub struct FileBookStore {
    dir: PathBuf,
    cache: std::collections::HashMap<ContentId, Vec<u8>>,
}

impl FileBookStore {
    /// Open (or create) a book store at the given directory path.
    pub fn open(dir: PathBuf) -> std::io::Result<Self> {
        todo!()
    }
}

impl BookStore for FileBookStore {
    fn insert_with_flags(
        &mut self,
        data: &[u8],
        flags: ContentFlags,
    ) -> Result<ContentId, harmony_content::error::ContentError> {
        todo!()
    }

    fn store(&mut self, cid: ContentId, data: Vec<u8>) {
        todo!()
    }

    fn get(&self, cid: &ContentId) -> Option<&[u8]> {
        todo!()
    }

    fn contains(&self, cid: &ContentId) -> bool {
        todo!()
    }

    fn remove(&mut self, cid: &ContentId) -> Option<Vec<u8>> {
        todo!()
    }
}

/// Encode a ContentId as a 64-character lowercase hex string.
pub fn cid_to_hex(cid: &ContentId) -> String {
    hex::encode(cid.to_bytes())
}

/// Decode a 64-character hex string into a ContentId.
pub fn hex_to_cid(s: &str) -> Result<ContentId, hex::FromHexError> {
    let bytes = hex::decode(s)?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| hex::FromHexError::InvalidStringLength)?;
    Ok(ContentId::from_bytes(arr))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, FileBookStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FileBookStore::open(dir.path().to_path_buf()).unwrap();
        (dir, store)
    }

    #[test]
    fn insert_and_get_round_trip() {
        let (_dir, mut store) = temp_store();
        let data = b"hello harmony cas";
        let cid = store.insert(data).unwrap();
        assert_eq!(store.get(&cid).unwrap(), data);
    }

    #[test]
    fn contains_after_insert() {
        let (_dir, mut store) = temp_store();
        let data = b"test contains";
        let cid = store.insert(data).unwrap();
        assert!(store.contains(&cid));
    }

    #[test]
    fn get_unknown_returns_none() {
        let (_dir, store) = temp_store();
        let cid = ContentId::for_book(b"not stored", ContentFlags::default()).unwrap();
        assert!(store.get(&cid).is_none());
        assert!(!store.contains(&cid));
    }

    #[test]
    fn remove_returns_data_and_deletes() {
        let (_dir, mut store) = temp_store();
        let data = b"to be removed";
        let cid = store.insert(data).unwrap();
        let removed = store.remove(&cid).unwrap();
        assert_eq!(removed, data);
        assert!(store.get(&cid).is_none());
        assert!(!store.contains(&cid));
    }

    #[test]
    fn persistence_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let data = b"persistent data";
        let cid;
        {
            let mut store = FileBookStore::open(dir.path().to_path_buf()).unwrap();
            cid = store.insert(data).unwrap();
        }
        // Reopen the store — should load from disk
        let store = FileBookStore::open(dir.path().to_path_buf()).unwrap();
        assert_eq!(store.get(&cid).unwrap(), data);
    }

    #[test]
    fn remove_deletes_file_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let data = b"will be removed from disk";
        let cid;
        {
            let mut store = FileBookStore::open(dir.path().to_path_buf()).unwrap();
            cid = store.insert(data).unwrap();
        }
        {
            let mut store = FileBookStore::open(dir.path().to_path_buf()).unwrap();
            store.remove(&cid);
        }
        // Reopen — should not find the book
        let store = FileBookStore::open(dir.path().to_path_buf()).unwrap();
        assert!(store.get(&cid).is_none());
    }

    #[test]
    fn cid_hex_round_trip() {
        let cid = ContentId::for_book(b"hex test", ContentFlags::default()).unwrap();
        let hex_str = cid_to_hex(&cid);
        assert_eq!(hex_str.len(), 64);
        let decoded = hex_to_cid(&hex_str).unwrap();
        assert_eq!(cid, decoded);
    }
}
```

Add `mod store;` to `main.rs`:

```rust
mod store;

fn main() {
    println!("cas-tool stub");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```bash
cd src-tauri && cargo test --features cas-tool --bin cas-tool
```

Expected: All 7 tests FAIL with `not yet implemented`.

- [ ] **Step 3: Implement FileBookStore**

Replace the `todo!()` stubs in `store.rs` with the real implementation:

```rust
impl FileBookStore {
    /// Open (or create) a book store at the given directory path.
    ///
    /// Eagerly loads all existing .book files into memory. This is fine for our
    /// scale (~16 files). New inserts write to both cache and disk.
    pub fn open(dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let mut cache = std::collections::HashMap::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "book") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(cid) = hex_to_cid(stem) {
                        let data = std::fs::read(&path)?;
                        cache.insert(cid, data);
                    }
                }
            }
        }
        Ok(FileBookStore { dir, cache })
    }

    fn book_path(&self, cid: &ContentId) -> PathBuf {
        self.dir.join(format!("{}.book", cid_to_hex(cid)))
    }
}

impl BookStore for FileBookStore {
    fn insert_with_flags(
        &mut self,
        data: &[u8],
        flags: ContentFlags,
    ) -> Result<ContentId, harmony_content::error::ContentError> {
        let cid = ContentId::for_book(data, flags)?;
        if !self.cache.contains_key(&cid) {
            std::fs::write(self.book_path(&cid), data)
                .expect("failed to write book to disk");
            self.cache.insert(cid, data.to_vec());
        }
        Ok(cid)
    }

    fn store(&mut self, cid: ContentId, data: Vec<u8>) {
        if !self.cache.contains_key(&cid) {
            let _ = std::fs::write(self.book_path(&cid), &data);
            self.cache.insert(cid, data);
        }
    }

    fn get(&self, cid: &ContentId) -> Option<&[u8]> {
        self.cache.get(cid).map(|v| v.as_slice())
    }

    fn contains(&self, cid: &ContentId) -> bool {
        self.cache.contains_key(cid)
    }

    fn remove(&mut self, cid: &ContentId) -> Option<Vec<u8>> {
        let data = self.cache.remove(cid)?;
        let _ = std::fs::remove_file(self.book_path(cid));
        Some(data)
    }
}
```

**Note on error mapping in `insert_with_flags`:** `ContentError` doesn't have an I/O variant. Disk write failure during ingest is unrecoverable, so use `.expect("failed to write book to disk")` on the I/O result. This panics with a clear message rather than misusing an unrelated error variant.

- [ ] **Step 4: Run tests to verify they pass**

Run:
```bash
cd src-tauri && cargo test --features cas-tool --bin cas-tool
```

Expected: All 7 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/bin/cas_tool/store.rs src-tauri/src/bin/cas_tool/main.rs
git commit -m "feat: add FileBookStore — disk-backed BookStore implementation"
```

---

### Task 3: Manifest — Load, save, and query filename→CID mappings

**Files:**
- Create: `src-tauri/src/bin/cas_tool/manifest.rs`
- Modify: `src-tauri/src/bin/cas_tool/main.rs` (add `mod manifest;`)

- [ ] **Step 1: Write failing tests for Manifest**

Create `src-tauri/src/bin/cas_tool/manifest.rs`:

```rust
use std::collections::BTreeMap;
use std::path::Path;

use harmony_content::cid::{ContentFlags, ContentId};
use serde::{Deserialize, Serialize};

use crate::store::{cid_to_hex, hex_to_cid};

/// A manifest mapping filenames to their ContentId hex strings.
///
/// Serialized as `{"files": {"name.png": "64-char hex", ...}}`.
/// Keys are sorted (BTreeMap) for deterministic output.
#[derive(Serialize, Deserialize, Debug)]
pub struct Manifest {
    pub files: BTreeMap<String, String>,
}

impl Manifest {
    /// Create an empty manifest.
    pub fn new() -> Self {
        Manifest {
            files: BTreeMap::new(),
        }
    }

    /// Load a manifest from a JSON file. Returns an empty manifest if the file doesn't exist.
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        todo!()
    }

    /// Save the manifest to a JSON file, creating parent directories if needed.
    pub fn save(&self, path: &Path) -> Result<(), ManifestError> {
        todo!()
    }

    /// Set a file entry. Returns the previous CID hex if the entry existed.
    pub fn set(&mut self, filename: String, cid: &ContentId) -> Option<String> {
        self.files.insert(filename, cid_to_hex(cid))
    }

    /// Get the ContentId for a filename.
    pub fn get_cid(&self, filename: &str) -> Result<Option<ContentId>, ManifestError> {
        match self.files.get(filename) {
            None => Ok(None),
            Some(hex_str) => {
                let cid = hex_to_cid(hex_str).map_err(|_| ManifestError::InvalidHex {
                    filename: filename.to_string(),
                    hex: hex_str.clone(),
                })?;
                Ok(Some(cid))
            }
        }
    }
}

#[derive(Debug)]
pub enum ManifestError {
    Io(std::io::Error),
    Json(serde_json::Error),
    InvalidHex { filename: String, hex: String },
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Io(e) => write!(f, "I/O error: {e}"),
            ManifestError::Json(e) => write!(f, "JSON error: {e}"),
            ManifestError::InvalidHex { filename, hex } => {
                write!(f, "invalid CID hex for '{filename}': '{hex}'")
            }
        }
    }
}

impl From<std::io::Error> for ManifestError {
    fn from(e: std::io::Error) -> Self {
        ManifestError::Io(e)
    }
}

impl From<serde_json::Error> for ManifestError {
    fn from(e: serde_json::Error) -> Self {
        ManifestError::Json(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_manifest_is_empty() {
        let m = Manifest::new();
        assert!(m.files.is_empty());
    }

    #[test]
    fn set_and_get_cid() {
        let mut m = Manifest::new();
        let cid = ContentId::for_book(b"test data", ContentFlags::default()).unwrap();
        m.set("test.png".to_string(), &cid);
        let got = m.get_cid("test.png").unwrap().unwrap();
        assert_eq!(cid, got);
    }

    #[test]
    fn get_unknown_returns_none() {
        let m = Manifest::new();
        assert!(m.get_cid("nope.png").unwrap().is_none());
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-manifest.json");

        let mut m = Manifest::new();
        let cid = ContentId::for_book(b"round trip", ContentFlags::default()).unwrap();
        m.set("file.png".to_string(), &cid);
        m.save(&path).unwrap();

        let loaded = Manifest::load(&path).unwrap();
        assert_eq!(loaded.get_cid("file.png").unwrap().unwrap(), cid);
    }

    #[test]
    fn load_nonexistent_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let m = Manifest::load(&path).unwrap();
        assert!(m.files.is_empty());
    }

    #[test]
    fn save_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("manifest.json");
        let m = Manifest::new();
        m.save(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn manifest_json_is_sorted() {
        let mut m = Manifest::new();
        let cid_a = ContentId::for_book(b"aaa", ContentFlags::default()).unwrap();
        let cid_b = ContentId::for_book(b"bbb", ContentFlags::default()).unwrap();
        // Insert in reverse order
        m.set("zebra.png".to_string(), &cid_b);
        m.set("apple.json".to_string(), &cid_a);

        let json = serde_json::to_string_pretty(&m).unwrap();
        let apple_pos = json.find("apple.json").unwrap();
        let zebra_pos = json.find("zebra.png").unwrap();
        assert!(apple_pos < zebra_pos, "keys should be sorted alphabetically");
    }
}
```

Add `mod manifest;` to `main.rs`:

```rust
mod manifest;
mod store;

fn main() {
    println!("cas-tool stub");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```bash
cd src-tauri && cargo test --features cas-tool --bin cas-tool
```

Expected: New manifest tests FAIL with `not yet implemented`. Store tests still pass.

- [ ] **Step 3: Implement Manifest load/save**

Replace the `todo!()` stubs in `manifest.rs`:

```rust
impl Manifest {
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        if !path.exists() {
            return Ok(Manifest::new());
        }
        let contents = std::fs::read_to_string(path)?;
        let manifest: Manifest = serde_json::from_str(&contents)?;
        Ok(manifest)
    }

    pub fn save(&self, path: &Path) -> Result<(), ManifestError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:
```bash
cd src-tauri && cargo test --features cas-tool --bin cas-tool
```

Expected: All 14 tests PASS (7 store + 7 manifest).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/bin/cas_tool/manifest.rs src-tauri/src/bin/cas_tool/main.rs
git commit -m "feat: add Manifest type — filename-to-CID mapping with JSON persistence"
```

---

### Task 4: cas-tool ingest subcommand

**Files:**
- Modify: `src-tauri/src/bin/cas_tool/main.rs`

- [ ] **Step 1: Write failing test for ingest logic**

Add an `ingest` function and test to `main.rs`. The ingest function is the core logic, separate from CLI parsing, so it's testable:

```rust
mod manifest;
mod store;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use crate::manifest::{Manifest, ManifestError};
use crate::store::FileBookStore;
use harmony_content::book::BookStore;

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
        /// Input directory containing files to ingest
        #[arg(long)]
        input: PathBuf,
        /// Output manifest JSON file
        #[arg(long)]
        manifest: PathBuf,
        /// Book store directory (default: XDG cache)
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Restore files from a manifest using the book store
    Restore {
        /// Manifest JSON file
        #[arg(long)]
        manifest: PathBuf,
        /// Output directory to write restored files
        #[arg(long)]
        output: PathBuf,
        /// Book store directory (default: XDG cache)
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
    todo!()
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Ingest { input, manifest, store } => {
            let store_dir = store.unwrap_or_else(default_store_dir);
            let mut book_store = FileBookStore::open(store_dir)
                .expect("failed to open book store");
            let result = ingest(&input, &manifest, &mut book_store)
                .expect("ingest failed");
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

    #[test]
    fn ingest_produces_correct_manifest() {
        let input_dir = tempfile::tempdir().unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let manifest_path = store_dir.path().join("manifest.json");

        // Create two input files
        std::fs::write(input_dir.path().join("a.png"), b"png data here").unwrap();
        std::fs::write(input_dir.path().join("b.json"), b"json data here").unwrap();

        let mut store = FileBookStore::open(store_dir.path().join("books")).unwrap();
        let result = ingest(input_dir.path(), &manifest_path, &mut store).unwrap();

        assert_eq!(result.total, 2);
        assert_eq!(result.new, 2);
        assert_eq!(result.unchanged, 0);

        // Verify manifest was written with correct CIDs
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert_eq!(manifest.files.len(), 2);
        assert!(manifest.files.contains_key("a.png"));
        assert!(manifest.files.contains_key("b.json"));

        // Verify CIDs match the data
        let cid_a = manifest.get_cid("a.png").unwrap().unwrap();
        assert!(cid_a.verify_hash(b"png data here"));
    }

    #[test]
    fn ingest_idempotent_on_unchanged_files() {
        let input_dir = tempfile::tempdir().unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let manifest_path = store_dir.path().join("manifest.json");

        std::fs::write(input_dir.path().join("file.png"), b"same data").unwrap();

        let mut store = FileBookStore::open(store_dir.path().join("books")).unwrap();

        // First ingest
        let r1 = ingest(input_dir.path(), &manifest_path, &mut store).unwrap();
        assert_eq!(r1.new, 1);

        // Second ingest — same data, should be unchanged
        let r2 = ingest(input_dir.path(), &manifest_path, &mut store).unwrap();
        assert_eq!(r2.new, 0);
        assert_eq!(r2.unchanged, 1);
    }

    #[test]
    fn ingest_detects_changed_file() {
        let input_dir = tempfile::tempdir().unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let manifest_path = store_dir.path().join("manifest.json");

        std::fs::write(input_dir.path().join("file.png"), b"version 1").unwrap();
        let mut store = FileBookStore::open(store_dir.path().join("books")).unwrap();
        ingest(input_dir.path(), &manifest_path, &mut store).unwrap();

        // Change the file
        std::fs::write(input_dir.path().join("file.png"), b"version 2").unwrap();
        let r = ingest(input_dir.path(), &manifest_path, &mut store).unwrap();
        assert_eq!(r.new, 1);
        assert_eq!(r.unchanged, 0);

        // Verify manifest updated
        let manifest = Manifest::load(&manifest_path).unwrap();
        let cid = manifest.get_cid("file.png").unwrap().unwrap();
        assert!(cid.verify_hash(b"version 2"));
    }

    #[test]
    fn ingest_empty_directory_errors() {
        let input_dir = tempfile::tempdir().unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let manifest_path = store_dir.path().join("manifest.json");

        let mut store = FileBookStore::open(store_dir.path().join("books")).unwrap();
        let result = ingest(input_dir.path(), &manifest_path, &mut store);
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```bash
cd src-tauri && cargo test --features cas-tool --bin cas-tool
```

Expected: 4 new tests FAIL with `not yet implemented`.

- [ ] **Step 3: Implement ingest function**

Replace the `todo!()` in the `ingest` function:

```rust
fn ingest(
    input_dir: &Path,
    manifest_path: &Path,
    store: &mut FileBookStore,
) -> Result<IngestResult, Box<dyn std::error::Error>> {
    if !input_dir.is_dir() {
        return Err(format!("input directory does not exist: {}", input_dir.display()).into());
    }

    // Collect files (non-recursive, skip directories)
    let mut files: Vec<_> = std::fs::read_dir(input_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .collect();
    files.sort_by_key(|e| e.file_name());

    if files.is_empty() {
        return Err(format!("no files found in {}", input_dir.display()).into());
    }

    let mut manifest = Manifest::load(manifest_path)?;
    let mut new = 0usize;
    let mut unchanged = 0usize;

    for entry in &files {
        let filename = entry.file_name().to_string_lossy().to_string();
        let data = std::fs::read(entry.path())?;
        let cid = store.insert(&data)?;
        let hex = crate::store::cid_to_hex(&cid);

        if manifest.files.get(&filename) == Some(&hex) {
            unchanged += 1;
        } else {
            manifest.set(filename, &cid);
            new += 1;
        }
    }

    manifest.save(manifest_path)?;

    Ok(IngestResult {
        total: files.len(),
        new,
        unchanged,
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:
```bash
cd src-tauri && cargo test --features cas-tool --bin cas-tool
```

Expected: All 18 tests PASS (7 store + 7 manifest + 4 ingest).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/bin/cas_tool/main.rs
git commit -m "feat: add cas-tool ingest subcommand — files to book store + manifest"
```

---

### Task 5: cas-tool restore subcommand

**Files:**
- Modify: `src-tauri/src/bin/cas_tool/main.rs`

- [ ] **Step 1: Write failing tests for restore**

Add a `restore` function and tests to `main.rs`:

```rust
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
    todo!()
}
```

Add these tests to the existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn restore_round_trip() {
        let input_dir = tempfile::tempdir().unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let output_dir = tempfile::tempdir().unwrap();
        let manifest_path = store_dir.path().join("manifest.json");

        // Create input files and ingest
        std::fs::write(input_dir.path().join("atlas.png"), b"png bytes").unwrap();
        std::fs::write(input_dir.path().join("atlas.json"), b"json bytes").unwrap();
        let mut store = FileBookStore::open(store_dir.path().join("books")).unwrap();
        ingest(input_dir.path(), &manifest_path, &mut store).unwrap();

        // Restore to a different directory
        let result = restore(&manifest_path, output_dir.path(), &store).unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.written, 2);
        assert_eq!(result.skipped, 0);

        // Verify restored files match originals
        assert_eq!(
            std::fs::read(output_dir.path().join("atlas.png")).unwrap(),
            b"png bytes"
        );
        assert_eq!(
            std::fs::read(output_dir.path().join("atlas.json")).unwrap(),
            b"json bytes"
        );
    }

    #[test]
    fn restore_skips_existing_correct_size() {
        let input_dir = tempfile::tempdir().unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let output_dir = tempfile::tempdir().unwrap();
        let manifest_path = store_dir.path().join("manifest.json");

        std::fs::write(input_dir.path().join("file.png"), b"data").unwrap();
        let mut store = FileBookStore::open(store_dir.path().join("books")).unwrap();
        ingest(input_dir.path(), &manifest_path, &mut store).unwrap();

        // Pre-create the output file with correct size
        std::fs::write(output_dir.path().join("file.png"), b"data").unwrap();

        let result = restore(&manifest_path, output_dir.path(), &store).unwrap();
        assert_eq!(result.skipped, 1);
        assert_eq!(result.written, 0);
    }

    #[test]
    fn restore_missing_cid_errors() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("manifest.json");

        // Write a manifest referencing a CID that doesn't exist in the store
        let mut m = Manifest::new();
        let cid = harmony_content::cid::ContentId::for_book(
            b"not in store",
            harmony_content::cid::ContentFlags::default(),
        )
        .unwrap();
        m.set("missing.png".to_string(), &cid);
        m.save(&manifest_path).unwrap();

        let store = FileBookStore::open(dir.path().join("empty-store")).unwrap();
        let result = restore(&manifest_path, dir.path().join("output").as_path(), &store);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing.png"), "error should name the file: {err}");
    }

    #[test]
    fn restore_creates_output_directory() {
        let input_dir = tempfile::tempdir().unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let manifest_path = store_dir.path().join("manifest.json");

        std::fs::write(input_dir.path().join("file.png"), b"data").unwrap();
        let mut store = FileBookStore::open(store_dir.path().join("books")).unwrap();
        ingest(input_dir.path(), &manifest_path, &mut store).unwrap();

        let output_dir = store_dir.path().join("new").join("nested").join("dir");
        let result = restore(&manifest_path, &output_dir, &store).unwrap();
        assert_eq!(result.written, 1);
        assert!(output_dir.join("file.png").exists());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```bash
cd src-tauri && cargo test --features cas-tool --bin cas-tool
```

Expected: 4 new restore tests FAIL with `not yet implemented`.

- [ ] **Step 3: Implement restore function**

Replace the `todo!()`:

```rust
fn restore(
    manifest_path: &Path,
    output_dir: &Path,
    store: &FileBookStore,
) -> Result<RestoreResult, Box<dyn std::error::Error>> {
    if !manifest_path.exists() {
        return Err(format!("manifest does not exist: {}", manifest_path.display()).into());
    }

    let manifest = Manifest::load(manifest_path)?;

    if manifest.files.is_empty() {
        return Err("manifest contains no files".into());
    }

    // First pass: verify all CIDs exist before writing anything
    let mut entries = Vec::new();
    for (filename, hex_str) in &manifest.files {
        let cid = crate::store::hex_to_cid(hex_str).map_err(|_| {
            format!("invalid CID hex for '{filename}': '{hex_str}'")
        })?;
        if !store.contains(&cid) {
            return Err(format!(
                "missing book {} for file '{}'. Run the full pipeline to populate the store.",
                hex_str, filename
            )
            .into());
        }
        entries.push((filename.clone(), cid));
    }

    // Second pass: write files
    std::fs::create_dir_all(output_dir)?;

    let mut written = 0usize;
    let mut skipped = 0usize;

    for (filename, cid) in &entries {
        let output_path = output_dir.join(filename);
        let data = store.get(cid).expect("verified in first pass");

        // Skip if file exists with correct size
        if let Ok(meta) = std::fs::metadata(&output_path) {
            if meta.len() == data.len() as u64 {
                skipped += 1;
                continue;
            }
        }

        std::fs::write(&output_path, data)?;
        written += 1;
    }

    Ok(RestoreResult {
        total: entries.len(),
        written,
        skipped,
    })
}
```

Update the `Command::Restore` match arm in `main()`:

```rust
        Command::Restore { manifest, output, store } => {
            let store_dir = store.unwrap_or_else(default_store_dir);
            let book_store = FileBookStore::open(store_dir)
                .expect("failed to open book store");
            let result = restore(&manifest, &output, &book_store)
                .expect("restore failed");
            println!(
                "Restored {} files ({} written, {} already present)",
                result.total, result.written, result.skipped
            );
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run:
```bash
cd src-tauri && cargo test --features cas-tool --bin cas-tool
```

Expected: All 22 tests PASS (7 store + 7 manifest + 4 ingest + 4 restore).

- [ ] **Step 5: Run clippy**

Run:
```bash
cd src-tauri && cargo clippy --features cas-tool --bin cas-tool -- -D warnings
```

Expected: No warnings.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/bin/cas_tool/main.rs
git commit -m "feat: add cas-tool restore subcommand — manifest to files from book store"
```

---

### Task 6: Pipeline integration — npm scripts, .gitignore, manifests directory

**Files:**
- Modify: `package.json`
- Modify: `.gitignore`
- Create: `manifests/.gitkeep`

- [ ] **Step 1: Update .gitignore**

Add `assets/sprites/` to `.gitignore`. Place it in the "Asset pipeline intermediate files" section:

```gitignore
# Asset pipeline intermediate files
tools/asset-pipeline/extracted/

# Asset pipeline outputs (restored from CAS)
assets/sprites/
```

- [ ] **Step 2: Add npm scripts to package.json**

Add these scripts to the `"scripts"` object in `package.json`:

```json
"ingest-items": "cargo run --manifest-path src-tauri/Cargo.toml --features cas-tool --bin cas-tool -- ingest --input assets/sprites/items --manifest manifests/items.json",
"restore-items": "cargo run --manifest-path src-tauri/Cargo.toml --features cas-tool --bin cas-tool -- restore --manifest manifests/items.json --output assets/sprites/items"
```

Update the existing `pipeline-items` script to include ingest:

```json
"pipeline-items": "npm run extract-items && npm run pack-items && npm run ingest-items"
```

- [ ] **Step 3: Create manifests directory**

```bash
mkdir -p manifests
touch manifests/.gitkeep
```

- [ ] **Step 4: Verify the tool runs from npm**

Run:
```bash
npm run ingest-items
```

Expected: Either succeeds (if `assets/sprites/items/` has files) or errors clearly about missing/empty input directory. Both are correct — the pipeline integration is wired up.

If `assets/sprites/items/` has the cherry.png placeholder:
```
Ingested 1 files (1 new, 0 unchanged)
```

- [ ] **Step 5: If ingest succeeded, verify restore**

Run:
```bash
rm assets/sprites/items/cherry.png
npm run restore-items
```

Expected:
```
Restored 1 files (1 written, 0 already present)
```

Verify the file was restored:
```bash
ls assets/sprites/items/
```

Expected: `cherry.png` is back.

- [ ] **Step 6: Remove atlas files from git tracking**

If `assets/sprites/` was previously tracked in git, remove it from tracking (the .gitignore will prevent re-adding):

```bash
git rm -r --cached assets/sprites/ 2>/dev/null || true
```

- [ ] **Step 7: Commit**

```bash
git add .gitignore package.json manifests/.gitkeep
git add manifests/items.json 2>/dev/null || true
git commit -m "feat: integrate cas-tool into asset pipeline — npm scripts, .gitignore, manifests"
```

---

### Task 7: End-to-end integration test

**Files:**
- Modify: `src-tauri/src/bin/cas_tool/main.rs` (add integration test)

- [ ] **Step 1: Write end-to-end test**

Add this test to the `#[cfg(test)] mod tests` block in `main.rs`:

```rust
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
```

- [ ] **Step 2: Run all tests**

Run:
```bash
cd src-tauri && cargo test --features cas-tool --bin cas-tool
```

Expected: All 23 tests PASS.

- [ ] **Step 3: Run clippy on the full project**

Run:
```bash
cd src-tauri && cargo clippy --features cas-tool --bin cas-tool -- -D warnings
```

Expected: No warnings.

- [ ] **Step 4: Verify frontend tests still pass**

Run:
```bash
npx vitest run
```

Expected: All existing tests pass (no frontend changes were made).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/bin/cas_tool/main.rs
git commit -m "test: add end-to-end integration test for cas-tool ingest/restore round-trip"
```
