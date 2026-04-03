use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use harmony_content::cid::ContentId;
use serde::{Deserialize, Serialize};

use crate::store::{cid_to_hex, hex_to_cid};

#[derive(Serialize, Deserialize, Debug)]
pub struct Manifest {
    pub files: BTreeMap<String, String>,
}

#[derive(Debug)]
pub enum ManifestError {
    Io(std::io::Error),
    Json(serde_json::Error),
    InvalidHex { filename: String, hex: String },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Io(e) => write!(f, "I/O error: {}", e),
            ManifestError::Json(e) => write!(f, "JSON error: {}", e),
            ManifestError::InvalidHex { filename, hex } => {
                write!(f, "invalid hex CID for '{}': '{}'", filename, hex)
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

impl Manifest {
    pub fn new() -> Self {
        Manifest {
            files: BTreeMap::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                let manifest: Manifest = serde_json::from_str(&contents)?;
                Ok(manifest)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Manifest::new()),
            Err(e) => Err(ManifestError::Io(e)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), ManifestError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn set(&mut self, filename: String, cid: &ContentId) -> Option<String> {
        self.files.insert(filename, cid_to_hex(cid))
    }

    pub fn get_cid(&self, filename: &str) -> Result<Option<ContentId>, ManifestError> {
        match self.files.get(filename) {
            None => Ok(None),
            Some(hex) => match hex_to_cid(hex) {
                Ok(cid) => Ok(Some(cid)),
                Err(_) => Err(ManifestError::InvalidHex {
                    filename: filename.to_owned(),
                    hex: hex.clone(),
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harmony_content::cid::{ContentFlags, ContentId};

    fn make_cid(data: &[u8]) -> ContentId {
        ContentId::for_book(data, ContentFlags::default()).unwrap()
    }

    #[test]
    fn new_manifest_is_empty() {
        let m = Manifest::new();
        assert!(m.files.is_empty());
    }

    #[test]
    fn set_and_get_cid() {
        let mut m = Manifest::new();
        let cid = make_cid(b"test content");
        m.set("foo.png".to_string(), &cid);
        let retrieved = m.get_cid("foo.png").unwrap().unwrap();
        assert_eq!(cid, retrieved);
    }

    #[test]
    fn get_unknown_returns_none() {
        let m = Manifest::new();
        let result = m.get_cid("nonexistent.png").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let mut m = Manifest::new();
        let cid1 = make_cid(b"file one");
        let cid2 = make_cid(b"file two");
        m.set("alpha.png".to_string(), &cid1);
        m.set("beta.png".to_string(), &cid2);
        m.save(&path).unwrap();

        let loaded = Manifest::load(&path).unwrap();
        assert_eq!(loaded.get_cid("alpha.png").unwrap().unwrap(), cid1);
        assert_eq!(loaded.get_cid("beta.png").unwrap().unwrap(), cid2);
    }

    #[test]
    fn load_nonexistent_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does_not_exist.json");
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
        // Insert in reverse alphabetical order
        m.set("zebra.png".to_string(), &make_cid(b"z"));
        m.set("mango.png".to_string(), &make_cid(b"m"));
        m.set("apple.png".to_string(), &make_cid(b"a"));

        let json = serde_json::to_string_pretty(&m).unwrap();
        let apple_pos = json.find("apple.png").unwrap();
        let mango_pos = json.find("mango.png").unwrap();
        let zebra_pos = json.find("zebra.png").unwrap();
        assert!(apple_pos < mango_pos, "apple should come before mango");
        assert!(mango_pos < zebra_pos, "mango should come before zebra");
    }
}
