# CAS Local Storage — Content-Addressed Asset Pipeline

**Issue:** harmony-glitch-irt
**Date:** 2026-04-02
**Status:** Approved

## Overview

Integrate harmony-content's content-addressed storage into the asset pipeline. Large asset blobs (sprite atlas PNGs and JSONs) move out of git into a local book store on disk. The repo tracks lightweight JSON manifests mapping filenames to ContentId hex strings. A new `cas-tool` Rust binary handles ingestion and restoration.

The game runtime is unchanged — `cas-tool restore` materializes files into `assets/sprites/` where PixiJS already reads them. CAS is purely a storage/distribution layer beneath the existing pipeline.

## Relationship to harmony-content

harmony-content provides the CAS primitives: `ContentId` (32-byte identifiers with SHA-224 hash), `BookStore` trait (insert/get/contains/remove), and `ContentFlags`. This bead builds the missing piece — a filesystem-backed `BookStore` — and wires it into the asset pipeline via a CLI tool.

We use harmony-content's types and hashing directly. When harmony-content gains its own disk storage or P2P distribution (StorageTier), our tool can adopt those capabilities with minimal changes because the `ContentId` format is the same.

### harmony-content API Surface Used

```rust
// From harmony_content::cid
pub struct ContentId {
    pub header: [u8; 4],
    pub hash: [u8; 28],
}
impl ContentId {
    pub fn for_book(data: &[u8], flags: ContentFlags) -> Result<Self, ContentError>;
    pub fn to_bytes(&self) -> [u8; 32];
    pub fn from_bytes(bytes: [u8; 32]) -> Self;
    pub fn verify_hash(&self, data: &[u8]) -> bool;
}

// From harmony_content::book
pub trait BookStore {
    fn insert_with_flags(&mut self, data: &[u8], flags: ContentFlags) -> Result<ContentId, ContentError>;
    fn insert(&mut self, data: &[u8]) -> Result<ContentId, ContentError>;
    fn store(&mut self, cid: ContentId, data: Vec<u8>);
    fn get(&self, cid: &ContentId) -> Option<&[u8]>;
    fn contains(&self, cid: &ContentId) -> bool;
    fn remove(&mut self, cid: &ContentId) -> Option<Vec<u8>>;
}
```

## FileBookStore

### Location

Default: `$XDG_CACHE_HOME/harmony-glitch/cas/` (falls back to `~/.cache/harmony-glitch/cas/` on Linux, `~/Library/Caches/harmony-glitch/cas/` on macOS, `%LOCALAPPDATA%\harmony-glitch\cas\` on Windows). Overridable via `--store` CLI flag.

### On-Disk Layout

Books stored as flat files named by their full hex CID (header + hash = 32 bytes = 64 hex chars):

```
~/.cache/harmony-glitch/cas/
  a1b2c3d4...64chars.book
  b7c8d9e0...64chars.book
```

No subdirectory fanout — the Glitch corpus produces ~8 atlas groups x 2 files = ~16 books. Fanout is premature complexity at this scale.

### BookStore Trait Implementation

`BookStore::get` returns `Option<&[u8]>` (borrowed), which requires the data to outlive the borrow. `FileBookStore` maintains an internal read cache (`HashMap<ContentId, Vec<u8>>`) and returns borrows into it:

```rust
pub struct FileBookStore {
    dir: PathBuf,
    cache: HashMap<ContentId, Vec<u8>>,
}
```

- `insert_with_flags(data, flags)` — Computes `ContentId::for_book(data, flags)`, writes `data` to `{dir}/{hex_cid}.book`, inserts into cache, returns CID.
- `get(cid)` — If not in cache, reads `{dir}/{hex_cid}.book` into cache. Returns `Some(&data)` or `None` if file doesn't exist.
- `contains(cid)` — Checks cache, then checks file existence on disk.
- `remove(cid)` — Removes from cache and deletes file from disk.
- `store(cid, data)` — Writes data to `{dir}/{hex_cid}.book` and inserts into cache. Used for pre-computed CIDs.

The hex CID is the full 32-byte `ContentId::to_bytes()` encoded as lowercase hex (64 characters).

### Interior Mutability for `get`

`BookStore::get(&self, ...)` takes `&self` but we need to mutate the cache on a miss. Given our small scale (~16 files), the simplest approach is to eagerly load all books into the cache on construction. `get` then borrows directly from the `HashMap` with no interior mutability needed.

```rust
impl FileBookStore {
    pub fn open(dir: PathBuf) -> Result<Self> {
        // Create dir if needed, then load all .book files into cache
    }
}
```

If the store directory has no `.book` files (fresh machine), the cache starts empty and `get` returns `None`. New books added via `insert` go into both cache and disk.

## cas-tool CLI

### Binary Target

```toml
[[bin]]
name = "cas-tool"
path = "src/bin/cas_tool/main.rs"
required-features = ["cas-tool"]
```

New feature in `[features]`:
```toml
cas-tool = ["clap", "dep:harmony-content"]
```

`clap` is already an optional dependency (shared with extract-swf). `harmony-content` is added as an optional dependency.

### Subcommands

#### `cas-tool ingest`

```
cas-tool ingest --input <DIR> --manifest <FILE> [--store <DIR>]
```

1. Scan `--input` directory for all files (non-recursive, any extension).
2. Read existing manifest from `--manifest` if it exists.
3. For each file:
   a. Read file contents.
   b. Compute `ContentId::for_book(data, ContentFlags::default())`.
   c. If CID matches the existing manifest entry for this filename, skip (already ingested).
   d. Otherwise, write to book store and update manifest entry.
4. Write updated manifest JSON to `--manifest`.
5. Print summary: `Ingested N files (M new, K unchanged)`.

#### `cas-tool restore`

```
cas-tool restore --manifest <FILE> --output <DIR> [--store <DIR>]
```

1. Read manifest JSON from `--manifest`.
2. Create `--output` directory if it doesn't exist.
3. For each `(filename, hex_cid)` entry:
   a. Parse hex CID to `ContentId`.
   b. If output file exists and has the correct byte length, skip.
   c. Read book from store via `BookStore::get(cid)`.
   d. If missing, error: `"Missing book {hex_cid} for file {filename}. Run the full pipeline to populate the store."`.
   e. Write book data to `--output/filename`.
4. Print summary: `Restored N files (M written, K already present)`.

Restore is all-or-nothing per manifest — if any CID is missing, it errors before writing anything. This is implemented by checking all CIDs exist before writing any files.

### Store Path Resolution

Default store path resolution order:
1. `--store` CLI flag (if provided)
2. `$XDG_CACHE_HOME/harmony-glitch/cas/` (if `$XDG_CACHE_HOME` is set)
3. `~/.cache/harmony-glitch/cas/` (Linux/macOS fallback)

The store directory is created (including parents) if it doesn't exist.

## Manifest Format

```json
{
  "files": {
    "items.png": "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2",
    "items.json": "b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8"
  }
}
```

- Keys are filenames (no paths, just the basename).
- Values are 64-character lowercase hex strings (32-byte ContentId).
- File is sorted by key for deterministic output and clean diffs.
- One manifest per atlas group (e.g., `manifests/items.json`).

Manifests live in `manifests/` at the repo root, committed to git.

## Pipeline Integration

### npm Scripts

```json
{
  "extract-items": "(unchanged)",
  "pack-items": "(unchanged)",
  "ingest-items": "cargo run --manifest-path src-tauri/Cargo.toml --features cas-tool --bin cas-tool -- ingest --input assets/sprites/items --manifest manifests/items.json",
  "restore-items": "cargo run --manifest-path src-tauri/Cargo.toml --features cas-tool --bin cas-tool -- restore --manifest manifests/items.json --output assets/sprites/items",
  "pipeline-items": "npm run extract-items && npm run pack-items && npm run ingest-items"
}
```

### Git Changes

- Add `assets/sprites/` to `.gitignore` (atlas PNGs and JSONs no longer tracked)
- Remove existing atlas files from git tracking (`git rm --cached assets/sprites/items/*`)
- Add `manifests/` directory to the repo (tracked)

### Developer Workflows

**Running the pipeline** (source SWFs changed or packer updated):
```
npm run pipeline-items
# Produces atlas files, ingests into CAS, updates manifest
git add manifests/items.json
git commit -m "update items manifest"
```

**Fresh clone / switching branches:**
```
npm run restore-items
# Materializes atlas files from local book store
```

**Empty cache (new machine, first time):**
```
npm run pipeline-items
# Full pipeline populates both assets/ and the book store
```

### Bootstrap Problem

On a truly fresh machine with no book store, `restore` fails because there are no books. The developer must run the full pipeline once (which requires the Glitch SWF archive at `$GLITCH_ART_PATH`). This matches today's workflow — you already need the archive to generate assets. P2P book distribution (follow-up bead) eliminates this requirement.

## Testing

### Rust Tests (cargo test)

- **FileBookStore insert + get:** Insert data, get returns identical bytes
- **FileBookStore contains:** Returns true after insert, false for unknown CID
- **FileBookStore remove:** Returns data, subsequent get returns None, file deleted
- **FileBookStore persistence:** Insert, drop store, create new store at same path, get returns data
- **Ingest produces correct manifest:** Ingest directory with 2 files, manifest has correct CIDs
- **Ingest idempotency:** Running ingest twice on unchanged files produces identical manifest
- **Ingest detects changes:** Modify a file, re-ingest, manifest updates only the changed entry
- **Restore round-trip:** Ingest directory → delete original files → restore from manifest → files match byte-for-byte
- **Restore missing CID:** Error with descriptive message naming the CID and filename
- **Restore skips existing:** Files already present with correct size are not overwritten
- **CID hex encoding round-trip:** ContentId → hex string → parse back → identical ContentId

### No Frontend Tests

cas-tool is a pure Rust CLI tool. No changes to pack.mjs or frontend code.

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Ingest: input directory doesn't exist | Error with path |
| Ingest: input directory is empty | Error: no files to ingest |
| Ingest: file read fails (permissions) | Error with filename |
| Restore: manifest doesn't exist | Error with path |
| Restore: CID missing from store | Error naming the CID + filename |
| Restore: output directory doesn't exist | Create it (including parents) |
| Restore: file write fails | Error with filename |
| Store directory doesn't exist | Create it (including parents) |
| Book file corrupted on read | Verify hash, error with CID if mismatch |
| Manifest JSON parse failure | Error with path and parse error |
| Invalid hex CID in manifest | Error with the key and hex string |

## Dependencies

### New Cargo Dependencies

```toml
harmony-content = { path = "../../harmony/crates/harmony-content", optional = true }
```

### New Feature

```toml
cas-tool = ["clap", "dep:harmony-content"]
```

No new external crates. `clap` is already present (shared with extract-swf). `hex` is already a dependency (used for identity).

## Out of Scope

- P2P asset distribution (follow-up bead)
- Tauri runtime reading from CAS directly (follow-up bead)
- Bundle/DAG structure for multi-file groups (YAGNI at ~16 files)
- Ingesting non-sprite assets (audio, street data) — same tool works, just not wired up yet
- Cache eviction in FileBookStore (not needed at this scale)
- W-TinyLFU / ContentStore (for network cache tier, not disk storage)
- Multi-frame SWF animation extraction (separate bead)

## Follow-Up Beads

- **CAS Runtime Integration** — Tauri app reads assets from book store via IPC, eliminating the `assets/sprites/` directory entirely. Filed as follow-up per design discussion.
- **P2P Asset Distribution** — Peers fetch books from the Harmony network. StorageTier integration with Zenoh queries.
