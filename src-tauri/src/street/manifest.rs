use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreetManifest {
    pub version: u32,
    pub streets: HashMap<String, StreetEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreetEntry {
    pub name: String,
    pub filename: String,
}

impl StreetManifest {
    /// Load manifest from a JSON file, returning an empty manifest if missing or invalid.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
                eprintln!("[streets] Failed to parse manifest: {e}");
                Self::empty()
            }),
            Err(_) => Self::empty(),
        }
    }

    pub fn empty() -> Self {
        Self {
            version: 1,
            streets: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_empty_manifest() {
        let m = StreetManifest::empty();
        assert_eq!(m.version, 1);
        assert!(m.streets.is_empty());
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let m = StreetManifest::load(Path::new("/nonexistent/manifest.json"));
        assert!(m.streets.is_empty());
    }

    #[test]
    fn roundtrip_manifest() {
        let mut m = StreetManifest::empty();
        m.streets.insert(
            "LA5101HF7F429V5".to_string(),
            StreetEntry {
                name: "Empty Via 5".to_string(),
                filename: "LA5101HF7F429V5.xml".to_string(),
            },
        );

        let json = serde_json::to_string(&m).unwrap();
        let loaded: StreetManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.streets.len(), 1);
        assert_eq!(loaded.streets["LA5101HF7F429V5"].name, "Empty Via 5");
    }
}
