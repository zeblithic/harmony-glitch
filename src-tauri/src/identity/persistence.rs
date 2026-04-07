use harmony_identity::{IdentityProof, PrivateIdentity, PuzzleParams};
use serde::{Deserialize, Serialize};
use std::path::Path;
use zeroize::Zeroizing;

#[derive(Clone, Serialize, Deserialize)]
pub struct PlayerProfile {
    pub identity_hex: String,
    /// Proof-of-work for the identity (Argon2id hashcash).
    /// `None` in legacy profiles triggers regeneration.
    #[serde(default)]
    pub identity_proof: Option<IdentityProof>,
    pub display_name: String,
    /// Whether the user has completed first-run identity setup.
    /// Defaults to false for backward compatibility with existing profiles.
    #[serde(default)]
    pub setup_complete: bool,
}

/// Redact private key material from debug output.
impl std::fmt::Debug for PlayerProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayerProfile")
            .field("identity_hex", &"[REDACTED]")
            .field("display_name", &self.display_name)
            .field("setup_complete", &self.setup_complete)
            .finish()
    }
}

/// Write profile JSON to disk with restrictive permissions (0600 on Unix).
/// Private key material lives in this file — it should not be world-readable.
///
/// Uses atomic write (temp → fsync → rename) so the profile is never corrupted
/// by a crash mid-write. On Unix, permissions are set on the temp file before
/// rename — no TOCTOU window.
pub fn write_profile(path: &Path, json: &str) -> Result<(), String> {
    let mode = if cfg!(unix) { Some(0o600) } else { None };
    crate::persistence::atomic_write(path, json.as_bytes(), mode)
}

/// Try to load an existing profile from disk. Returns an error if the file
/// is missing, malformed, contains invalid key material, or has no valid
/// proof-of-work (legacy profiles without proofs are treated as invalid).
///
/// Intermediate key bytes are wrapped in `Zeroizing` so they are wiped
/// from memory on drop, matching the codebase's key-material hygiene.
fn try_load_profile(
    path: &Path,
    params: &PuzzleParams,
) -> Result<(PrivateIdentity, IdentityProof, String, bool), String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let profile: PlayerProfile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let raw = Zeroizing::new(hex::decode(&profile.identity_hex).map_err(|e| e.to_string())?);
    let identity = PrivateIdentity::from_private_bytes(&raw).map_err(|e| format!("{e:?}"))?;

    // Require a valid proof-of-work. Legacy profiles (None) trigger regeneration.
    let proof = profile
        .identity_proof
        .ok_or("profile has no identity proof")?;
    if !identity.verify_proof(&proof, params) {
        return Err("identity proof is invalid".into());
    }

    Ok((identity, proof, profile.display_name, profile.setup_complete))
}

/// Load or create a player profile. Creates directory and new identity if none exists.
/// If an existing profile is corrupted or missing a valid proof-of-work, logs the
/// error and generates a fresh identity with proof.
/// Returns (identity, proof, display_name, setup_complete).
pub fn load_or_create_profile(
    data_dir: &Path,
    params: &PuzzleParams,
) -> Result<(PrivateIdentity, IdentityProof, String, bool), String> {
    let profile_path = data_dir.join("profile.json");

    if profile_path.exists() {
        match try_load_profile(&profile_path, params) {
            Ok(result) => return Ok(result),
            Err(e) => {
                eprintln!("[identity] Failed to load profile ({}); regenerating.", e);
                // Fall through to generate a fresh profile.
            }
        }
    }

    {
        let mut rng = rand::rngs::OsRng;
        let (identity, proof) = PrivateIdentity::generate_with_proof(&mut rng, params);
        let addr_hash = identity.public_identity().address_hash;
        let display_name = format!("Glitchen_{}", &hex::encode(addr_hash)[..6]);

        std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        let identity_hex = Zeroizing::new(hex::encode(identity.to_private_bytes()));
        let profile = PlayerProfile {
            identity_hex: (*identity_hex).clone(),
            identity_proof: Some(proof),
            display_name: display_name.clone(),
            setup_complete: false,
        };
        let json = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
        write_profile(&profile_path, &json)?;

        Ok((identity, proof, display_name, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const TEST_PARAMS: PuzzleParams = PuzzleParams::TEST;

    #[test]
    fn creates_new_profile_when_none_exists() {
        let dir = TempDir::new().unwrap();
        let (identity, proof, name, setup_complete) =
            load_or_create_profile(dir.path(), &TEST_PARAMS).unwrap();
        assert!(name.starts_with("Glitchen_"));
        assert!(!setup_complete);
        assert!(dir.path().join("profile.json").exists());
        assert_eq!(identity.public_identity().address_hash.len(), 16);
        assert!(identity.verify_proof(&proof, &TEST_PARAMS));
    }

    #[test]
    #[cfg(unix)]
    fn profile_has_restrictive_permissions() {
        let dir = TempDir::new().unwrap();
        load_or_create_profile(dir.path(), &TEST_PARAMS).unwrap();
        let metadata = std::fs::metadata(dir.path().join("profile.json")).unwrap();
        let mode = std::os::unix::fs::PermissionsExt::mode(&metadata.permissions());
        assert_eq!(
            mode & 0o777,
            0o600,
            "profile.json should be owner-only (0600)"
        );
    }

    #[test]
    fn corrupted_profile_regenerates_identity() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(dir.path().join("profile.json"), "not valid json!!!").unwrap();

        // Should succeed by generating a fresh identity, not crash.
        let (identity, proof, name, setup_complete) =
            load_or_create_profile(dir.path(), &TEST_PARAMS).unwrap();
        assert!(name.starts_with("Glitchen_"));
        assert!(!setup_complete);
        assert_eq!(identity.public_identity().address_hash.len(), 16);
        assert!(identity.verify_proof(&proof, &TEST_PARAMS));
    }

    #[test]
    fn loads_existing_profile_with_same_identity() {
        let dir = TempDir::new().unwrap();
        let (id1, proof1, name1, _) = load_or_create_profile(dir.path(), &TEST_PARAMS).unwrap();
        let (id2, proof2, name2, _) = load_or_create_profile(dir.path(), &TEST_PARAMS).unwrap();
        assert_eq!(name1, name2);
        assert_eq!(
            id1.public_identity().address_hash,
            id2.public_identity().address_hash
        );
        assert_eq!(proof1, proof2);
    }

    #[test]
    fn profile_without_proof_regenerates() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path()).unwrap();

        // Write a legacy profile with no proof field.
        let mut rng = rand::rngs::OsRng;
        let identity = PrivateIdentity::generate(&mut rng);
        let identity_hex = hex::encode(identity.to_private_bytes());
        let json = serde_json::json!({
            "identity_hex": identity_hex,
            "display_name": "LegacyPlayer",
            "setup_complete": true,
        });
        std::fs::write(
            dir.path().join("profile.json"),
            serde_json::to_string_pretty(&json).unwrap(),
        )
        .unwrap();

        // Loading should regenerate because proof is missing.
        let (new_id, proof, name, setup_complete) =
            load_or_create_profile(dir.path(), &TEST_PARAMS).unwrap();
        // Regenerated → different identity, fresh name, setup_complete reset.
        assert_ne!(
            new_id.public_identity().address_hash,
            identity.public_identity().address_hash
        );
        assert!(name.starts_with("Glitchen_"));
        assert!(!setup_complete);
        assert!(new_id.verify_proof(&proof, &TEST_PARAMS));
    }

    #[test]
    fn profile_with_invalid_proof_regenerates() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path()).unwrap();

        // Write a profile with a fabricated (invalid) proof.
        let mut rng = rand::rngs::OsRng;
        let (identity, _valid_proof) =
            PrivateIdentity::generate_with_proof(&mut rng, &TEST_PARAMS);
        let identity_hex = hex::encode(identity.to_private_bytes());
        let tampered_proof = IdentityProof {
            nonce: 0xDEADBEEF,
            difficulty: 99,
            params_version: TEST_PARAMS.params_version,
        };
        let profile = PlayerProfile {
            identity_hex,
            identity_proof: Some(tampered_proof),
            display_name: "Tampered".into(),
            setup_complete: true,
        };
        std::fs::write(
            dir.path().join("profile.json"),
            serde_json::to_string_pretty(&profile).unwrap(),
        )
        .unwrap();

        // Loading should regenerate because proof doesn't verify.
        let (new_id, proof, _name, setup_complete) =
            load_or_create_profile(dir.path(), &TEST_PARAMS).unwrap();
        assert_ne!(
            new_id.public_identity().address_hash,
            identity.public_identity().address_hash
        );
        assert!(!setup_complete);
        assert!(new_id.verify_proof(&proof, &TEST_PARAMS));
    }
}
