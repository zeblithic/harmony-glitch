use harmony_identity::PrivateIdentity;
use serde::{Deserialize, Serialize};
use std::path::Path;
use zeroize::Zeroizing;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerProfile {
    pub identity_hex: String,
    pub display_name: String,
    /// Whether the user has completed first-run identity setup.
    /// Defaults to false for backward compatibility with existing profiles.
    #[serde(default)]
    pub setup_complete: bool,
}

/// Write profile JSON to disk with restrictive permissions (0600 on Unix).
/// Private key material lives in this file — it should not be world-readable.
///
/// On Unix, the file is created with 0600 permissions atomically (no TOCTOU window).
/// On Windows, default ACLs apply; OS-keychain integration is deferred.
pub fn write_profile(path: &Path, json: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| e.to_string())?;
        f.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, json).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Try to load an existing profile from disk. Returns an error if the file
/// is missing, malformed, or contains invalid key material.
///
/// Intermediate key bytes are wrapped in `Zeroizing` so they are wiped
/// from memory on drop, matching the codebase's key-material hygiene.
fn try_load_profile(path: &Path) -> Result<(PrivateIdentity, String, bool), String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let profile: PlayerProfile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let raw = Zeroizing::new(hex::decode(&profile.identity_hex).map_err(|e| e.to_string())?);
    let identity = PrivateIdentity::from_private_bytes(&raw).map_err(|e| format!("{e:?}"))?;
    Ok((identity, profile.display_name, profile.setup_complete))
}

/// Load or create a player profile. Creates directory and new identity if none exists.
/// If an existing profile is corrupted, logs the error and generates a fresh identity.
/// Returns (identity, display_name, setup_complete).
pub fn load_or_create_profile(
    data_dir: &Path,
) -> Result<(PrivateIdentity, String, bool), String> {
    let profile_path = data_dir.join("profile.json");

    if profile_path.exists() {
        match try_load_profile(&profile_path) {
            Ok(result) => return Ok(result),
            Err(e) => {
                eprintln!(
                    "[identity] Failed to load profile ({}); regenerating.",
                    e
                );
                // Fall through to generate a fresh profile.
            }
        }
    }

    {
        let mut rng = rand::rngs::OsRng;
        let identity = PrivateIdentity::generate(&mut rng);
        let addr_hash = identity.public_identity().address_hash;
        let display_name = format!("Glitchen_{}", &hex::encode(addr_hash)[..6]);

        std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        let profile = PlayerProfile {
            identity_hex: hex::encode(identity.to_private_bytes()),
            display_name: display_name.clone(),
            setup_complete: false,
        };
        let json = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
        write_profile(&profile_path, &json)?;

        Ok((identity, display_name, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn creates_new_profile_when_none_exists() {
        let dir = TempDir::new().unwrap();
        let (identity, name, setup_complete) = load_or_create_profile(dir.path()).unwrap();
        assert!(name.starts_with("Glitchen_"));
        assert!(!setup_complete);
        assert!(dir.path().join("profile.json").exists());
        assert_eq!(identity.public_identity().address_hash.len(), 16);
    }

    #[test]
    #[cfg(unix)]
    fn profile_has_restrictive_permissions() {
        let dir = TempDir::new().unwrap();
        load_or_create_profile(dir.path()).unwrap();
        let metadata = std::fs::metadata(dir.path().join("profile.json")).unwrap();
        let mode = std::os::unix::fs::PermissionsExt::mode(&metadata.permissions());
        assert_eq!(mode & 0o777, 0o600, "profile.json should be owner-only (0600)");
    }

    #[test]
    fn corrupted_profile_regenerates_identity() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(dir.path().join("profile.json"), "not valid json!!!").unwrap();

        // Should succeed by generating a fresh identity, not crash.
        let (identity, name, setup_complete) = load_or_create_profile(dir.path()).unwrap();
        assert!(name.starts_with("Glitchen_"));
        assert!(!setup_complete);
        assert_eq!(identity.public_identity().address_hash.len(), 16);
    }

    #[test]
    fn loads_existing_profile_with_same_identity() {
        let dir = TempDir::new().unwrap();
        let (id1, name1, _) = load_or_create_profile(dir.path()).unwrap();
        let (id2, name2, _) = load_or_create_profile(dir.path()).unwrap();
        assert_eq!(name1, name2);
        assert_eq!(
            id1.public_identity().address_hash,
            id2.public_identity().address_hash
        );
    }
}
