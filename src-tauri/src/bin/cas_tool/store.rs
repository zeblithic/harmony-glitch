use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use harmony_content::book::BookStore;
use harmony_content::cid::{ContentFlags, ContentId};
use harmony_content::error::ContentError;

/// A disk-backed content-addressed store for book data.
///
/// On construction, eagerly loads all existing `.book` files from the given directory
/// into an in-memory HashMap cache. All subsequent reads serve from the cache.
/// Inserts write to both cache and disk.
pub struct FileBookStore {
    dir: PathBuf,
    cache: HashMap<ContentId, Vec<u8>>,
}

impl FileBookStore {
    /// Open (or create) a FileBookStore at the given directory path.
    ///
    /// Creates the directory if it does not exist. Scans for `*.book` files and
    /// loads each one into the in-memory cache.
    pub fn open(dir: PathBuf) -> Self {
        fs::create_dir_all(&dir).expect("failed to create book store directory");

        let mut cache = HashMap::new();

        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("book") {
                    continue;
                }
                let stem = match path.file_stem().and_then(|s| s.to_str()) {
                    Some(s) => s.to_owned(),
                    None => continue,
                };
                let cid = match hex_to_cid(&stem) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let data = match fs::read(&path) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                cache.insert(cid, data);
            }
        }

        FileBookStore { dir, cache }
    }

    /// Compute the path for a book file given its ContentId.
    fn book_path(&self, cid: &ContentId) -> PathBuf {
        self.dir.join(format!("{}.book", cid_to_hex(cid)))
    }
}

impl BookStore for FileBookStore {
    fn insert_with_flags(
        &mut self,
        data: &[u8],
        flags: ContentFlags,
    ) -> Result<ContentId, ContentError> {
        let cid = ContentId::for_book(data, flags)?;
        if !self.cache.contains_key(&cid) {
            let path = self.book_path(&cid);
            fs::write(&path, data).expect("failed to write book to disk");
            self.cache.insert(cid, data.to_vec());
        }
        Ok(cid)
    }

    fn store(&mut self, cid: ContentId, data: Vec<u8>) {
        if !self.cache.contains_key(&cid) {
            let path = self.book_path(&cid);
            fs::write(&path, &data).expect("failed to write book to disk");
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
        let path = self.book_path(cid);
        let _ = fs::remove_file(&path);
        Some(data)
    }
}

/// Convert a ContentId to a 64-character lowercase hex string.
pub fn cid_to_hex(cid: &ContentId) -> String {
    hex::encode(cid.to_bytes())
}

/// Parse a 64-character lowercase hex string back to a ContentId.
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
    use harmony_content::book::BookStore;

    #[test]
    fn insert_and_get_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = FileBookStore::open(dir.path().to_path_buf());
        let data = b"hello harmony cas";
        let cid = store.insert(data).unwrap();
        assert_eq!(store.get(&cid).unwrap(), data);
    }

    #[test]
    fn contains_after_insert() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = FileBookStore::open(dir.path().to_path_buf());
        let cid = store.insert(b"some data for contains test").unwrap();
        assert!(store.contains(&cid));
    }

    #[test]
    fn get_unknown_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileBookStore::open(dir.path().to_path_buf());
        let cid = ContentId::for_book(b"not stored", ContentFlags::default()).unwrap();
        assert!(store.get(&cid).is_none());
        assert!(!store.contains(&cid));
    }

    #[test]
    fn remove_returns_data_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = FileBookStore::open(dir.path().to_path_buf());
        let data = b"data to be removed";
        let cid = store.insert(data).unwrap();
        let removed = store.remove(&cid).unwrap();
        assert_eq!(removed, data);
        assert!(store.get(&cid).is_none());
    }

    #[test]
    fn persistence_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let data = b"persisted data";
        let cid = {
            let mut store = FileBookStore::open(path.clone());
            store.insert(data).unwrap()
        };
        // Drop the store, then reopen at same path.
        let store2 = FileBookStore::open(path);
        assert_eq!(store2.get(&cid).unwrap(), data);
    }

    #[test]
    fn remove_deletes_file_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let data = b"file to delete from disk";
        let cid = {
            let mut store = FileBookStore::open(path.clone());
            store.insert(data).unwrap()
        };
        // Reopen, remove, then reopen again.
        {
            let mut store2 = FileBookStore::open(path.clone());
            store2.remove(&cid);
        }
        let store3 = FileBookStore::open(path);
        assert!(store3.get(&cid).is_none());
    }

    #[test]
    fn cid_hex_round_trip() {
        let cid = ContentId::for_book(b"round trip hex test", ContentFlags::default()).unwrap();
        let hex_str = cid_to_hex(&cid);
        assert_eq!(hex_str.len(), 64);
        let cid2 = hex_to_cid(&hex_str).unwrap();
        assert_eq!(cid, cid2);
    }
}
