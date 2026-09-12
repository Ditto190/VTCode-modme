//! Batch memory extraction across past sessions.
//!
//! Codex-style two-phase pipeline completion: per-session grounded facts have
//! already been extracted into each session's `derived/memory.json` view (see
//! [`super::finalize_persistent_memory`]); this module re-reads those views
//! from the recent sessions, dedupes them, and feeds them through the normal
//! classify → consolidate path under the global [`MemoryLock`](super::MemoryLock)
//! so the user-level `memory_summary.md` reflects the whole session history,
//! not only the most recent one.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use tokio::sync::Semaphore;

use super::{FactsInput, GroundedFactRecord, PersistentMemoryWriteReport, RuntimeAgentConfig, VTCodeConfig};

/// Facts kept per session during batch collection. Mirrors the eviction
/// summary's grounded-fact cap so no single session dominates the batch.
const FACTS_PER_SESSION: usize = 32;

/// Report for one batch extraction run.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchMemoryReport {
    /// Sessions scanned (most recent first, bounded by `max_sessions`).
    pub sessions_scanned: usize,
    /// Sessions that contributed at least one fact.
    pub sessions_with_facts: usize,
    /// Unique candidate facts fed into classification.
    pub candidate_facts: usize,
    /// Sessions skipped because they were still marked active.
    pub active_sessions_skipped: usize,
    /// Underlying write report when consolidation ran.
    pub write_report: Option<PersistentMemoryWriteReport>,
}

/// Extract memory candidates from the most recent `max_sessions` sessions and
/// consolidate them into the persistent memory store.
///
/// Session reads are parallelized with a `concurrency`-sized semaphore; the
/// classify + consolidate LLM work runs once over the merged candidates under
/// the global memory lock. Safe to run while a session finalization is in
/// flight: the lock serializes the mutating phase.
pub async fn run_batch_memory_extraction(
    runtime_config: &RuntimeAgentConfig,
    vt_cfg: Option<&VTCodeConfig>,
    workspace_root: &Path,
    max_sessions: usize,
    concurrency: usize,
) -> Result<BatchMemoryReport> {
    if max_sessions == 0 {
        bail!("batch memory extraction requires max_sessions > 0");
    }
    let config = super::effective_generated_memory_config(vt_cfg);
    if !config.enabled {
        bail!(
            "persistent memory generation is disabled; enable `[agent.persistent_memory] enabled` (and memories) first"
        );
    }

    let summaries = vtcode_memory::recent_sessions(workspace_root, max_sessions);
    let semaphore = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut handles = Vec::with_capacity(summaries.len());
    for summary in summaries {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .context("batch memory extraction semaphore closed")?;
        let workspace = workspace_root.to_path_buf();
        let session_id = summary.session_id.clone();
        let active = summary.status == "active";
        handles.push(tokio::task::spawn_blocking(move || {
            let _permit = permit;
            match vtcode_memory::session_memory_facts(&workspace, &session_id, FACTS_PER_SESSION) {
                Ok(facts) => (session_id, active, facts),
                Err(error) => {
                    tracing::warn!(session_id = %session_id, error = %error, "batch memory: failed to read session facts; skipping");
                    (session_id, active, Vec::new())
                }
            }
        }));
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut candidates: Vec<GroundedFactRecord> = Vec::new();
    let mut sessions_scanned = 0usize;
    let mut sessions_with_facts = 0usize;
    let mut active_sessions_skipped = 0usize;
    for handle in handles {
        let Ok((session_id, active, facts)) = handle.await else {
            continue;
        };
        sessions_scanned += 1;
        if active {
            // A live session's view is still being written; skip it to avoid
            // ingesting a partial snapshot.
            active_sessions_skipped += 1;
            continue;
        }
        let mut contributed = false;
        for fact in facts {
            let normalized = super::normalize_whitespace(&fact.fact).to_ascii_lowercase();
            if normalized.is_empty() || !seen.insert(normalized) {
                continue;
            }
            candidates.push(GroundedFactRecord {
                fact: fact.fact,
                source: format!("session:{session_id}"),
            });
            contributed = true;
        }
        if contributed {
            sessions_with_facts += 1;
        }
    }

    let candidate_count = candidates.len();
    let write_report = if candidate_count == 0 {
        None
    } else {
        super::persist_memory_internal(
            &config,
            workspace_root,
            Some(runtime_config),
            vt_cfg,
            FactsInput::Candidates(&candidates),
            true,
            false,
        )
        .await
        .context("batch memory consolidation")?
    };

    Ok(BatchMemoryReport {
        sessions_scanned,
        sessions_with_facts,
        candidate_facts: candidate_count,
        active_sessions_skipped,
        write_report,
    })
}

/// Resolve the batch parameters from config with a sanity clamp.
pub fn batch_parameters(config: &crate::config::PersistentMemoryConfig) -> (usize, usize) {
    let sessions = config.memories.batch_sessions.max(1);
    let concurrency = config.memories.batch_concurrency.clamp(1, 16);
    (sessions, concurrency)
}

/// Resolve batch parameters from an optional loaded config, applying the
/// documented defaults when no config is available.
pub fn batch_parameters_from_config(config: Option<&crate::config::PersistentMemoryConfig>) -> (usize, usize) {
    match config {
        Some(config) => batch_parameters(config),
        None => batch_parameters(&crate::config::PersistentMemoryConfig::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_parameters_clamp_degenerate_config() {
        let mut config = crate::config::PersistentMemoryConfig::default();
        config.memories.batch_sessions = 0;
        config.memories.batch_concurrency = 999;
        let (sessions, concurrency) = batch_parameters(&config);
        assert_eq!(sessions, 1);
        assert_eq!(concurrency, 16);
    }

    #[test]
    fn batch_parameters_use_defaults() {
        let config = crate::config::PersistentMemoryConfig::default();
        let (sessions, concurrency) = batch_parameters(&config);
        assert_eq!(sessions, 50);
        assert_eq!(concurrency, 8);
    }
}
