//! Digest-verified audit packs for sessions.
//!
//! An audit pack is a self-describing manifest of one session's canonical
//! store: every file under the session directory with its byte count and
//! SHA-256, plus a snapshot of the manifest counters. It is the portable
//! artifact behind VT Code's "auditable agent" story — a reviewer can take
//! `events.jsonl` + `manifest.json` + derived views, re-run
//! [`verify_audit_pack`], and confirm the artifacts are exactly the ones the
//! pack describes (nothing mutated, nothing missing, anything new reported as
//! unaccounted).
//!
//! Packs only reference file paths relative to the session directory; paths
//! are validated on load so a hand-edited pack cannot make verification read
//! outside the session directory.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::error::SessionStoreError;
use crate::{ensure_private_directory, session_dir};

/// Reserved file name for audit packs inside a session directory; excluded
/// from pack walks so a pack never describes itself.
const AUDIT_PACK_FILE_NAME: &str = "audit-pack.json";

/// One digest-verified file entry, relative to the session directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditPackEntry {
    /// Slash-separated path relative to the session directory (never
    /// absolute, never containing `..`).
    pub path: String,
    /// File size in bytes.
    pub bytes: u64,
    /// SHA-256 hex digest of the file contents.
    pub sha256: String,
}

/// Point-in-time digest manifest of a session's store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAuditPack {
    /// Pack schema version (see [`AUDIT_PACK_SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// Session the pack describes.
    pub session_id: String,
    /// RFC3339 creation time; pins the snapshot point.
    pub generated_at: String,
    /// Manifest status snapshot (`active` / `completed`).
    pub status: String,
    /// Manifest turn-count snapshot.
    pub turn_count: u64,
    /// Manifest event-count snapshot.
    pub event_count: u64,
    /// File entries, sorted by `path` for deterministic, diffable output.
    pub entries: Vec<AuditPackEntry>,
}

/// Outcome of verifying a pack against the current session contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditVerification {
    /// True when every listed file matches its digest and none are missing.
    pub verified: bool,
    /// Files whose current digest differs from the pack.
    pub mismatches: Vec<String>,
    /// Files listed in the pack that no longer exist.
    pub missing: Vec<String>,
    /// Files present in the session directory that the pack does not describe
    /// (informational: created after the pack, or excluded on purpose).
    pub unaccounted: Vec<String>,
}

/// Schema version for [`SessionAuditPack`].
pub const AUDIT_PACK_SCHEMA_VERSION: u32 = 1;

/// Default pack location inside the session store:
/// `<session>/derived/audit-pack.json`.
#[must_use]
pub fn audit_pack_path(workspace: &Path, session_id: &str) -> PathBuf {
    session_dir(workspace, session_id)
        .join(crate::DERIVED_DIR)
        .join(AUDIT_PACK_FILE_NAME)
}

/// Build a digest manifest of `session_id`'s store.
///
/// # Errors
/// Returns [`SessionStoreError`] when the session directory or its manifest
/// cannot be read, or when any file cannot be digested.
pub fn create_audit_pack(workspace: &Path, session_id: &str) -> Result<SessionAuditPack, SessionStoreError> {
    let dir = session_dir(workspace, session_id);
    let manifest_path = dir.join("manifest.json");
    let manifest_bytes =
        std::fs::read(&manifest_path).map_err(|error| SessionStoreError::io(manifest_path.clone(), error))?;
    let summary: crate::query::SessionSummary = serde_json::from_slice(&manifest_bytes)?;

    let mut entries = Vec::new();
    for file in walk_files(&dir)? {
        let relative = file
            .strip_prefix(&dir)
            .map_err(|error| SessionStoreError::io(file.clone(), std::io::Error::other(error)))?
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let (bytes, sha256) = digest_file(&file)?;
        entries.push(AuditPackEntry { path: relative, bytes, sha256 });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(SessionAuditPack {
        schema_version: AUDIT_PACK_SCHEMA_VERSION,
        session_id: session_id.to_string(),
        generated_at: Utc::now().to_rfc3339(),
        status: summary.status,
        turn_count: summary.turn_count,
        event_count: summary.event_count,
        entries,
    })
}

/// Write a pack as pretty JSON, atomically and with private permissions.
///
/// Defaults to [`audit_pack_path`] when `output` is `None`. The pack file is
/// excluded from the pack itself.
///
/// # Errors
/// Returns [`SessionStoreError`] from pack creation or the write.
pub fn write_audit_pack(
    workspace: &Path,
    session_id: &str,
    output: Option<&Path>,
) -> Result<(SessionAuditPack, PathBuf), SessionStoreError> {
    let pack = create_audit_pack(workspace, session_id)?;
    let destination = output.map_or_else(|| audit_pack_path(workspace, session_id), Path::to_path_buf);
    if let Some(parent) = destination.parent() {
        ensure_private_directory(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(&pack)?;
    vtcode_commons::VtCodePaths::write_private_file_atomic(&destination, &bytes)
        .map_err(|error| SessionStoreError::io(destination.clone(), std::io::Error::other(error)))?;
    Ok((pack, destination))
}

/// Load a pack from disk, validating its schema.
///
/// # Errors
/// Returns [`SessionStoreError::InvalidPack`] for unknown schema versions and
/// [`SessionStoreError::Io`]/[`SessionStoreError::Json`] for read or parse
/// failures.
pub fn read_audit_pack(path: &Path) -> Result<SessionAuditPack, SessionStoreError> {
    let bytes = std::fs::read(path).map_err(|error| SessionStoreError::io(path.to_path_buf(), error))?;
    let pack: SessionAuditPack = serde_json::from_slice(&bytes)?;
    if pack.schema_version != AUDIT_PACK_SCHEMA_VERSION {
        return Err(SessionStoreError::InvalidPack(format!(
            "unsupported schema_version {} (expected {AUDIT_PACK_SCHEMA_VERSION})",
            pack.schema_version
        )));
    }
    Ok(pack)
}

/// Verify a pack against the session's current contents.
///
/// A `false` `verified` means the store drifted from the pack: files changed
/// (mismatches), disappeared (missing), or — informationally — appeared after
/// the pack (unaccounted). Traversal-style entry paths are rejected outright.
///
/// # Errors
/// Returns [`SessionStoreError::InvalidPack`] for unsafe entry paths and
/// [`SessionStoreError`] for walk/digest IO failures.
pub fn verify_audit_pack(
    workspace: &Path,
    session_id: &str,
    pack: &SessionAuditPack,
) -> Result<AuditVerification, SessionStoreError> {
    let dir = session_dir(workspace, session_id);
    let mut mismatches = Vec::new();
    let mut missing = Vec::new();
    let mut listed = HashSet::new();

    for entry in &pack.entries {
        validate_entry_path(&entry.path)?;
        listed.insert(entry.path.clone());
        let absolute = dir.join(&entry.path);
        let (bytes, sha256) = match digest_file(&absolute) {
            Ok(digest) => digest,
            Err(error) if matches!(&error, SessionStoreError::Io { .. } if is_not_found(&error)) => {
                missing.push(entry.path.clone());
                continue;
            }
            Err(error) => return Err(error),
        };
        if bytes != entry.bytes || sha256 != entry.sha256 {
            mismatches.push(entry.path.clone());
        }
    }

    let mut unaccounted = Vec::new();
    for file in walk_files(&dir)? {
        let relative = file
            .strip_prefix(&dir)
            .map(|relative| {
                relative
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .map_err(|error| SessionStoreError::io(file.clone(), std::io::Error::other(error)))?;
        if !listed.contains(&relative) {
            unaccounted.push(relative);
        }
    }
    unaccounted.sort();

    Ok(AuditVerification {
        verified: mismatches.is_empty() && missing.is_empty(),
        mismatches,
        missing,
        unaccounted,
    })
}

/// Walk every regular file under `dir`, deterministically, skipping audit
/// packs themselves.
fn walk_files(dir: &Path) -> Result<Vec<PathBuf>, SessionStoreError> {
    let mut files = Vec::new();
    for entry in WalkDir::new(dir).sort_by_file_name() {
        let entry = entry.map_err(|error| SessionStoreError::io(dir.to_path_buf(), std::io::Error::other(error)))?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name().to_string_lossy() == AUDIT_PACK_FILE_NAME {
            continue;
        }
        files.push(entry.into_path());
    }
    Ok(files)
}

/// Digest a file, returning `(bytes, sha256-hex)`.
fn digest_file(path: &Path) -> Result<(u64, String), SessionStoreError> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).map_err(|error| SessionStoreError::io(path.to_path_buf(), error))?;
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| SessionStoreError::io(path.to_path_buf(), error))?;
        if read == 0 {
            break;
        }
        match buffer.get(..read) {
            Some(chunk) => hasher.update(chunk),
            // Unreachable: `read` never exceeds the buffer length.
            None => break,
        }
        bytes += u64::try_from(read).unwrap_or(u64::MAX);
    }
    let hex = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    Ok((bytes, hex))
}

/// Reject absolute paths, traversal components, and empty paths in packs.
fn validate_entry_path(relative: &str) -> Result<(), SessionStoreError> {
    if relative.is_empty() {
        return Err(SessionStoreError::InvalidPack("entry path is empty".to_string()));
    }
    let path = Path::new(relative);
    if path.is_absolute() {
        return Err(SessionStoreError::InvalidPack(format!("entry path {relative:?} is absolute")));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            other => {
                return Err(SessionStoreError::InvalidPack(format!(
                    "entry path {relative:?} contains a forbidden component ({other:?})"
                )));
            }
        }
    }
    Ok(())
}

fn is_not_found(error: &SessionStoreError) -> bool {
    matches!(
        error,
        SessionStoreError::Io { source, .. }
            if source.kind() == std::io::ErrorKind::NotFound
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Seed a session directory with a manifest and two files (root + derived)
    /// so pack walks cover both depths.
    fn seed_session(workspace: &Path, session_id: &str) -> PathBuf {
        let dir = session_dir(workspace, session_id);
        ensure_private_directory(&dir).expect("session dir");
        ensure_private_directory(&dir.join(crate::DERIVED_DIR)).expect("derived dir");
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::json!({
                "session_id": session_id,
                "schema_version": 1,
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z",
                "turn_count": 2,
                "event_count": 7,
                "status": "completed"
            })
            .to_string(),
        )
        .expect("manifest");
        std::fs::write(dir.join("events.jsonl"), "{\"event\":1}\n{\"event\":2}\n").expect("events");
        std::fs::write(dir.join(crate::DERIVED_DIR).join("memory.json"), "{\"facts\":[]}").expect("derived");
        dir
    }

    #[test]
    fn pack_round_trips_and_verifies_clean() {
        let workspace = TempDir::new().expect("workspace");
        seed_session(workspace.path(), "audit-a");

        let pack = create_audit_pack(workspace.path(), "audit-a").expect("create pack");
        // The derived/memory.json file name and manifest must be present; a
        // pack missing either would verify nothing meaningful.
        let paths: Vec<&str> = pack.entries.iter().map(|entry| entry.path.as_str()).collect();
        assert!(paths.contains(&"manifest.json"), "entries: {paths:?}");
        assert!(paths.contains(&"events.jsonl"), "entries: {paths:?}");
        assert!(paths.contains(&"derived/memory.json"), "entries: {paths:?}");

        // JSON round-trip must preserve the pack exactly (verify loads a file).
        let bytes = serde_json::to_vec_pretty(&pack).expect("serialize");
        let loaded: SessionAuditPack = serde_json::from_slice(&bytes).expect("deserialize");
        assert_eq!(loaded, pack);

        let report = verify_audit_pack(workspace.path(), "audit-a", &loaded).expect("verify");
        assert!(report.verified, "fresh pack must verify: {report:?}");
        assert!(report.mismatches.is_empty() && report.missing.is_empty() && report.unaccounted.is_empty());
        assert_eq!(
            report,
            AuditVerification {
                verified: true,
                mismatches: Vec::new(),
                missing: Vec::new(),
                unaccounted: Vec::new(),
            }
        );
    }

    #[test]
    fn single_byte_tamper_fails_verification() {
        let workspace = TempDir::new().expect("workspace");
        seed_session(workspace.path(), "audit-b");
        let pack = create_audit_pack(workspace.path(), "audit-b").expect("create pack");

        // Flip one byte of the canonical log: digest must change.
        let events = workspace.path().join(".vtcode/sessions/audit-b/events.jsonl");
        let contents = std::fs::read_to_string(&events).expect("read");
        std::fs::write(&events, contents.replace("{\"event\":1}", "{\"event\":9}")).expect("tamper");

        let report = verify_audit_pack(workspace.path(), "audit-b", &pack).expect("verify");
        assert!(!report.verified);
        assert_eq!(report.mismatches, vec!["events.jsonl".to_string()]);
        assert!(report.missing.is_empty());
    }

    #[test]
    fn appended_file_is_unaccounted_but_verifies() {
        let workspace = TempDir::new().expect("workspace");
        seed_session(workspace.path(), "audit-c");
        let pack = create_audit_pack(workspace.path(), "audit-c").expect("create pack");

        // A new file after the pack is informational, not a failure: the pack
        // pins its snapshot, it does not forbid later writes.
        std::fs::write(workspace.path().join(".vtcode/sessions/audit-c/derived/progress.json"), "{}")
            .expect("new file");

        let report = verify_audit_pack(workspace.path(), "audit-c", &pack).expect("verify");
        assert!(report.verified, "additions must not fail verification: {report:?}");
        assert_eq!(report.unaccounted, vec!["derived/progress.json".to_string()]);
    }

    #[test]
    fn deleted_file_is_reported_missing() {
        let workspace = TempDir::new().expect("workspace");
        seed_session(workspace.path(), "audit-d");
        let pack = create_audit_pack(workspace.path(), "audit-d").expect("create pack");

        std::fs::remove_file(workspace.path().join(".vtcode/sessions/audit-d/derived/memory.json"))
            .expect("delete derived file");

        let report = verify_audit_pack(workspace.path(), "audit-d", &pack).expect("verify");
        assert!(!report.verified);
        assert_eq!(report.missing, vec!["derived/memory.json".to_string()]);
        assert!(report.mismatches.is_empty(), "missing must not double-report as mismatched");
    }

    #[test]
    fn traversal_paths_in_loaded_packs_are_rejected() {
        let workspace = TempDir::new().expect("workspace");
        seed_session(workspace.path(), "audit-e");

        let malicious = |path: &str| SessionAuditPack {
            schema_version: AUDIT_PACK_SCHEMA_VERSION,
            session_id: "audit-e".to_string(),
            generated_at: "2026-01-01T00:00:00Z".to_string(),
            status: "completed".to_string(),
            turn_count: 0,
            event_count: 0,
            entries: vec![AuditPackEntry {
                path: path.to_string(),
                bytes: 1,
                sha256: "0".repeat(64),
            }],
        };
        for evil in ["../outside.json", "/etc/passwd", "derived/../../escape", ""] {
            let error = verify_audit_pack(workspace.path(), "audit-e", &malicious(evil))
                .expect_err("traversal pack must be rejected");
            assert!(
                matches!(error, SessionStoreError::InvalidPack(_)),
                "path {evil:?} must be an InvalidPack error, got {error:?}"
            );
        }
    }

    #[test]
    fn missing_session_fails_pack_creation() {
        let workspace = TempDir::new().expect("workspace");
        let error = create_audit_pack(workspace.path(), "ghost").expect_err("missing session");
        assert!(matches!(error, SessionStoreError::Io { .. }));
    }

    #[test]
    fn packs_are_deterministic_apart_from_timestamp() {
        let workspace = TempDir::new().expect("workspace");
        seed_session(workspace.path(), "audit-f");
        let first = create_audit_pack(workspace.path(), "audit-f").expect("first");
        let second = create_audit_pack(workspace.path(), "audit-f").expect("second");
        assert_eq!(first.entries, second.entries, "file inventory must be stable and sorted");
        let sorted: Vec<String> = second.entries.iter().map(|entry| entry.path.clone()).collect();
        let mut expected = sorted.clone();
        expected.sort();
        assert_eq!(sorted, expected);
    }

    #[test]
    fn write_and_read_round_trip_through_default_location() {
        let workspace = TempDir::new().expect("workspace");
        seed_session(workspace.path(), "audit-g");
        let (pack, path) = write_audit_pack(workspace.path(), "audit-g", None).expect("write pack");
        assert_eq!(path, audit_pack_path(workspace.path(), "audit-g"));

        let loaded = read_audit_pack(&path).expect("read pack");
        assert_eq!(loaded, pack);
        assert_eq!(loaded.schema_version, AUDIT_PACK_SCHEMA_VERSION);
        // The pack file itself must not appear in its own inventory.
        assert!(
            loaded.entries.iter().all(|entry| !entry.path.ends_with(AUDIT_PACK_FILE_NAME)),
            "pack must exclude itself: {:?}",
            loaded.entries
        );

        // Schema drift is rejected at load, not at verify time.
        let mut drifted = loaded.clone();
        drifted.schema_version = 99;
        std::fs::write(&path, serde_json::to_vec(&drifted).expect("serialize")).expect("write drifted");
        let error = read_audit_pack(&path).expect_err("schema drift");
        assert!(matches!(error, SessionStoreError::InvalidPack(_)));
    }
}
