//! Context reset logic for long-running harness sessions.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

const CONTEXT_RESET_DIR: &str = ".vtcode/tasks";
pub const CONTEXT_RESET_FILE: &str = "current_transition.json";
const LEGACY_CONTEXT_RESET_FILE: &str = "current_context_reset.md";
const TRANSITION_MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    CompactionReset,
    StallReset,
    LegacyReset,
}

/// Private durable handoff for compaction/reset recovery. This is deliberately
/// separate from the frozen public `ThreadEvent` schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionManifest {
    pub version: u32,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub transition_kind: TransitionKind,
    pub triggered_at: String,
    pub local_trigger: String,
    pub stall_count: u32,
    #[serde(default)]
    pub checkpoint_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact_boundary_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_reset_ref: Option<String>,
}

impl TransitionManifest {
    fn from_legacy(manifest: &ContextResetManifest) -> Self {
        let transition_kind = match manifest.trigger.as_str() {
            "compaction" => TransitionKind::CompactionReset,
            "stall" => TransitionKind::StallReset,
            _ => TransitionKind::LegacyReset,
        };
        Self {
            version: TRANSITION_MANIFEST_VERSION,
            thread_id: None,
            turn_id: None,
            transition_kind,
            triggered_at: manifest.triggered_at.clone(),
            local_trigger: manifest.trigger.clone(),
            stall_count: manifest.stall_count,
            checkpoint_paths: Vec::new(),
            compact_boundary_ref: None,
            context_reset_ref: None,
        }
    }

    fn as_legacy_reset(&self) -> ContextResetManifest {
        ContextResetManifest {
            triggered_at: self.triggered_at.clone(),
            trigger: self.local_trigger.clone(),
            stall_count: self.stall_count,
        }
    }

    pub fn orientation_text(&self) -> String {
        format!(
            "# Context Transition Manifest\n\n- Version: {}\n- Transition: {:?}\n- Triggered at: {}\n- Local trigger: {}\n- Stall count: {}\n\nThis session starts fresh from durable artifacts only. Reorient before applying the reset.",
            self.version, self.transition_kind, self.triggered_at, self.local_trigger, self.stall_count
        )
    }
}

pub async fn write_transition_manifest_async(workspace_root: &Path, manifest: &TransitionManifest) -> Result<bool> {
    anyhow::ensure!(
        manifest.version == TRANSITION_MANIFEST_VERSION,
        "unsupported transition manifest version {}",
        manifest.version
    );
    let dir = workspace_root.join(CONTEXT_RESET_DIR);
    tokio::fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("create context reset directory {}", dir.display()))?;
    let path = dir.join(CONTEXT_RESET_FILE);
    vtcode_commons::fs::write_private_json_file(&path, manifest)
        .await
        .with_context(|| format!("write context transition manifest {}", path.display()))?;
    Ok(true)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextResetDecision {
    Continue,
    Reset { reason: String, stall_count: u32 },
}

#[derive(Debug, Clone)]
pub struct ContextResetManifest {
    pub triggered_at: String,
    pub trigger: String,
    pub stall_count: u32,
}

impl ContextResetManifest {
    pub fn to_markdown(&self) -> String {
        format!(
            "# Context Reset Manifest\n\n - **Triggered at:** {}\n - **Trigger:** {}\n - **Stall count:** {}\n\nThis session starts fresh from external artifacts only.\nRead the current feature list and progress tracker to reorient.\n",
            self.triggered_at, self.trigger, self.stall_count
        )
    }
    pub fn from_markdown(md: &str) -> Option<Self> {
        /// Extract the value following `- **<key>:**` on a line, trimmed.
        fn field_value(line: &str, key: &str) -> Option<String> {
            let line = line.trim();
            let prefix = format!("- **{key}:**");
            line.strip_prefix(&prefix).map(|rest| rest.trim().to_string())
        }

        let mut a = String::new();
        let mut b = String::new();
        let mut c = 0u32;
        for line in md.lines() {
            if let Some(v) = field_value(line, "Triggered at") {
                a = v;
            } else if let Some(v) = field_value(line, "Trigger") {
                b = v;
            } else if let Some(v) = field_value(line, "Stall count") {
                c = v.parse().unwrap_or(0);
            }
        }
        if a.is_empty() || b.is_empty() {
            return None;
        }
        Some(Self { triggered_at: a, trigger: b, stall_count: c })
    }
}

pub fn should_reset(mode: &str, compaction: bool, stall: u32, threshold: u32) -> ContextResetDecision {
    match mode {
        "off" => ContextResetDecision::Continue,
        "on_compaction" => {
            if compaction {
                ContextResetDecision::Reset {
                    reason: "compaction triggered reset".into(),
                    stall_count: 0,
                }
            } else {
                ContextResetDecision::Continue
            }
        }
        "on_stall" => {
            if stall >= threshold && threshold > 0 {
                ContextResetDecision::Reset {
                    reason: format!("stall {stall}>={threshold}"),
                    stall_count: stall,
                }
            } else {
                ContextResetDecision::Continue
            }
        }
        _ => ContextResetDecision::Continue,
    }
}

pub fn write_manifest(workspace_root: &Path, manifest: &ContextResetManifest) -> Result<bool> {
    let dir = workspace_root.join(CONTEXT_RESET_DIR);
    std::fs::create_dir_all(&dir)?;
    let serialized = serde_json::to_vec_pretty(&TransitionManifest::from_legacy(manifest))
        .context("serialize context transition manifest")?;
    vtcode_commons::VtCodePaths::write_private_file_atomic(dir.join(CONTEXT_RESET_FILE), &serialized)?;
    Ok(true)
}

/// Write a context-reset manifest without blocking the async executor.
pub async fn write_manifest_async(workspace_root: &Path, manifest: &ContextResetManifest) -> Result<bool> {
    let transition = TransitionManifest::from_legacy(manifest);
    write_transition_manifest_async(workspace_root, &transition).await
}

pub fn read_transition_manifest(workspace_root: &Path) -> Result<Option<TransitionManifest>> {
    let directory = workspace_root.join(CONTEXT_RESET_DIR);
    let current = directory.join(CONTEXT_RESET_FILE);
    match std::fs::read(&current) {
        Ok(contents) => {
            let manifest: TransitionManifest = serde_json::from_slice(&contents)
                .with_context(|| format!("parse context transition manifest {}", current.display()))?;
            anyhow::ensure!(
                manifest.version == TRANSITION_MANIFEST_VERSION,
                "unsupported context transition manifest version {}",
                manifest.version
            );
            Ok(Some(manifest))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let legacy_path = directory.join(LEGACY_CONTEXT_RESET_FILE);
            match std::fs::read_to_string(&legacy_path) {
                Ok(legacy) => ContextResetManifest::from_markdown(&legacy)
                    .map(|manifest| Some(TransitionManifest::from_legacy(&manifest)))
                    .ok_or_else(|| anyhow::anyhow!("invalid legacy context reset manifest {}", legacy_path.display())),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => {
                    Err(error).with_context(|| format!("read legacy context reset manifest {}", legacy_path.display()))
                }
            }
        }
        Err(error) => Err(error).with_context(|| format!("read context transition manifest {}", current.display())),
    }
}

/// Read a pending transition manifest without blocking the async executor.
pub async fn read_transition_manifest_async(workspace_root: &Path) -> Result<Option<TransitionManifest>> {
    let directory = workspace_root.join(CONTEXT_RESET_DIR);
    let current = directory.join(CONTEXT_RESET_FILE);
    match tokio::fs::read(&current).await {
        Ok(contents) => {
            let manifest: TransitionManifest = serde_json::from_slice(&contents)
                .with_context(|| format!("parse context transition manifest {}", current.display()))?;
            anyhow::ensure!(
                manifest.version == TRANSITION_MANIFEST_VERSION,
                "unsupported context transition manifest version {}",
                manifest.version
            );
            Ok(Some(manifest))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let legacy_path = directory.join(LEGACY_CONTEXT_RESET_FILE);
            match tokio::fs::read_to_string(&legacy_path).await {
                Ok(legacy) => ContextResetManifest::from_markdown(&legacy)
                    .map(|manifest| Some(TransitionManifest::from_legacy(&manifest)))
                    .ok_or_else(|| anyhow::anyhow!("invalid legacy context reset manifest {}", legacy_path.display())),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => {
                    Err(error).with_context(|| format!("read legacy context reset manifest {}", legacy_path.display()))
                }
            }
        }
        Err(error) => Err(error).with_context(|| format!("read context transition manifest {}", current.display())),
    }
}

pub fn read_manifest(workspace_root: &Path) -> Option<ContextResetManifest> {
    read_transition_manifest(workspace_root)
        .ok()
        .flatten()
        .map(|manifest| manifest.as_legacy_reset())
}

pub fn read_manifest_orientation(workspace_root: &Path) -> Option<String> {
    read_transition_manifest(workspace_root)
        .ok()
        .flatten()
        .map(|manifest| manifest.orientation_text())
}

pub fn consume_manifest(workspace_root: &Path) -> Result<()> {
    let directory = workspace_root.join(CONTEXT_RESET_DIR);
    for path in [
        directory.join(CONTEXT_RESET_FILE),
        directory.join(LEGACY_CONTEXT_RESET_FILE),
    ] {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("consume context transition manifest {}", path.display()));
            }
        }
    }
    Ok(())
}

/// Consume a pending transition manifest without blocking the async executor.
pub async fn consume_manifest_async(workspace_root: &Path) -> Result<()> {
    let directory = workspace_root.join(CONTEXT_RESET_DIR);
    for path in [
        directory.join(CONTEXT_RESET_FILE),
        directory.join(LEGACY_CONTEXT_RESET_FILE),
    ] {
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("consume context transition manifest {}", path.display()));
            }
        }
    }
    Ok(())
}

/// The `stall_count` is 0 because compaction-triggered resets are not stall-driven;
/// the threshold check is enforced only for `on_stall` mode in `should_reset`.
pub fn maybe_write_reset_after_compaction(workspace_root: &Path, mode: &str) -> bool {
    if mode != "on_compaction" {
        return false;
    }
    let m = ContextResetManifest {
        triggered_at: chrono::Utc::now().to_rfc3339(),
        trigger: "compaction".into(),
        stall_count: 0,
    };
    write_manifest(workspace_root, &m).unwrap_or(false)
}

/// Write the compaction-triggered reset manifest without blocking the runtime.
pub async fn maybe_write_reset_after_compaction_async(workspace_root: &Path, mode: &str) -> Result<bool> {
    maybe_write_reset_after_compaction_with_context_async(workspace_root, mode, None, None, Vec::new(), None).await
}

pub async fn maybe_write_reset_after_compaction_with_context_async(
    workspace_root: &Path,
    mode: &str,
    thread_id: Option<String>,
    turn_id: Option<String>,
    checkpoint_paths: Vec<String>,
    compact_boundary_ref: Option<String>,
) -> Result<bool> {
    if mode != "on_compaction" {
        return Ok(false);
    }
    let manifest = TransitionManifest {
        version: TRANSITION_MANIFEST_VERSION,
        thread_id,
        turn_id,
        transition_kind: TransitionKind::CompactionReset,
        triggered_at: chrono::Utc::now().to_rfc3339(),
        local_trigger: "compaction".into(),
        stall_count: 0,
        checkpoint_paths,
        compact_boundary_ref,
        context_reset_ref: Some("context_reset:unknown".to_string()),
    };
    write_transition_manifest_async(workspace_root, &manifest).await
}

pub fn maybe_write_reset_on_stall(workspace_root: &Path, stall_count: u32, mode: &str, threshold: u32) {
    if let ContextResetDecision::Reset { stall_count: sc, .. } = should_reset(mode, false, stall_count, threshold) {
        let m = ContextResetManifest {
            triggered_at: chrono::Utc::now().to_rfc3339(),
            trigger: "stall".into(),
            stall_count: sc,
        };
        let _ = write_manifest(workspace_root, &m);
    }
}

/// Write the stall-triggered reset manifest without blocking the runtime.
pub async fn maybe_write_reset_on_stall_async(
    workspace_root: &Path,
    stall_count: u32,
    mode: &str,
    threshold: u32,
) -> Result<bool> {
    maybe_write_reset_on_stall_with_context_async(workspace_root, stall_count, mode, threshold, None, None).await
}

pub async fn maybe_write_reset_on_stall_with_context_async(
    workspace_root: &Path,
    stall_count: u32,
    mode: &str,
    threshold: u32,
    thread_id: Option<String>,
    turn_id: Option<String>,
) -> Result<bool> {
    let ContextResetDecision::Reset { stall_count: sc, .. } = should_reset(mode, false, stall_count, threshold) else {
        return Ok(false);
    };
    let manifest = TransitionManifest {
        version: TRANSITION_MANIFEST_VERSION,
        thread_id,
        turn_id,
        transition_kind: TransitionKind::StallReset,
        triggered_at: chrono::Utc::now().to_rfc3339(),
        local_trigger: "stall".into(),
        stall_count: sc,
        checkpoint_paths: Vec::new(),
        compact_boundary_ref: None,
        context_reset_ref: Some("context_reset:unknown".to_string()),
    };
    write_transition_manifest_async(workspace_root, &manifest).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    #[test]
    fn off_always_continues() {
        assert!(matches!(should_reset("off", true, 10, 2), ContextResetDecision::Continue));
    }
    #[test]
    fn compaction_triggers() {
        assert!(matches!(should_reset("on_compaction", true, 0, 0), ContextResetDecision::Reset { .. }));
    }
    #[test]
    fn stall_triggers_at_threshold() {
        assert!(matches!(should_reset("on_stall", false, 2, 2), ContextResetDecision::Reset { stall_count: 2, .. }));
    }
    #[test]
    fn manifest_round_trips_through_markdown() {
        let m = ContextResetManifest {
            triggered_at: "2026-01-01T00:00:00Z".into(),
            trigger: "stall".into(),
            stall_count: 3,
        };
        let md = m.to_markdown();
        let back = ContextResetManifest::from_markdown(&md).expect("round-trip");
        assert_eq!(back.triggered_at, m.triggered_at);
        assert_eq!(back.trigger, m.trigger);
        assert_eq!(back.stall_count, 3);
    }
    #[test]
    fn manifest_from_empty_is_none() {
        assert!(ContextResetManifest::from_markdown("").is_none());
    }

    #[tokio::test]
    async fn async_manifest_writer_preserves_manifest_contents() {
        let workspace = TempDir::new().expect("workspace");
        let manifest = ContextResetManifest {
            triggered_at: "2026-01-01T00:00:00Z".into(),
            trigger: "compaction".into(),
            stall_count: 0,
        };

        assert!(write_manifest_async(workspace.path(), &manifest).await.expect("write manifest"));
        let loaded = read_manifest(workspace.path()).expect("read manifest");
        assert_eq!(loaded.triggered_at, manifest.triggered_at);
        assert_eq!(loaded.trigger, manifest.trigger);
        assert_eq!(loaded.stall_count, manifest.stall_count);
    }

    #[tokio::test]
    async fn async_reset_helpers_only_write_for_matching_modes() {
        let workspace = TempDir::new().expect("workspace");

        assert!(
            !maybe_write_reset_after_compaction_async(workspace.path(), "off")
                .await
                .expect("compaction mode")
        );
        assert!(
            !maybe_write_reset_on_stall_async(workspace.path(), 1, "on_stall", 2)
                .await
                .expect("stall threshold")
        );
        assert!(read_manifest(workspace.path()).is_none());
    }

    #[test]
    fn malformed_transition_manifest_remains_retryable() {
        let workspace = TempDir::new().expect("workspace");
        let path = workspace.path().join(CONTEXT_RESET_DIR).join(CONTEXT_RESET_FILE);
        std::fs::create_dir_all(path.parent().expect("manifest parent")).expect("create manifest parent");
        std::fs::write(&path, b"{ malformed").expect("write malformed manifest");

        assert!(read_transition_manifest(workspace.path()).is_err());
        assert!(path.exists(), "failed reads must not consume the pending transition");
    }

    #[tokio::test]
    async fn transition_manifest_preserves_identity_and_checkpoint_references() {
        let workspace = TempDir::new().expect("workspace");
        let manifest = TransitionManifest {
            version: TRANSITION_MANIFEST_VERSION,
            thread_id: Some("thread-1".to_string()),
            turn_id: Some("turn-2".to_string()),
            transition_kind: TransitionKind::CompactionReset,
            triggered_at: "2026-01-01T00:00:00Z".to_string(),
            local_trigger: "compaction".to_string(),
            stall_count: 0,
            checkpoint_paths: vec!["history.md".to_string(), "memory.md".to_string()],
            compact_boundary_ref: Some("segment-2".to_string()),
            context_reset_ref: Some("context_reset:unknown".to_string()),
        };

        assert!(
            write_transition_manifest_async(workspace.path(), &manifest)
                .await
                .expect("write transition")
        );
        assert_eq!(read_transition_manifest_async(workspace.path()).await.expect("read transition"), Some(manifest));
        consume_manifest_async(workspace.path()).await.expect("consume transition");
        assert_eq!(
            read_transition_manifest_async(workspace.path())
                .await
                .expect("read consumed transition"),
            None
        );
    }
}
