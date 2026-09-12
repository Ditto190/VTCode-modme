//! `vtcode session-store` — operate the unified per-session state store.

use std::path::PathBuf;

use anyhow::{Context, Result};
use vtcode_core::cli::args::SessionStoreCommand;
use vtcode_memory::{
    DEFAULT_MAX_EVENTS, RetentionPolicy, apply_retention, migrate_legacy, open, query_facts, read_audit_pack,
    recent_sessions, verify_audit_pack, write_audit_pack,
};

/// Handle the `session-store` CLI subcommand.
pub async fn handle_session_store_command(command: SessionStoreCommand) -> Result<()> {
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    match command {
        SessionStoreCommand::Migrate { remove_legacy } => {
            let report =
                migrate_legacy(&workspace, remove_legacy).context("failed to migrate legacy session stores")?;
            println!(
                "Migrated {} sessions ({} memory envelopes, {} trajectories, {} bytes).",
                report.sessions_created, report.memory_imported, report.trajectory_imported, report.bytes_migrated
            );
            if remove_legacy {
                println!("Removed legacy history/ and logs/ directories.");
            }
        }
        SessionStoreCommand::Gc { max_sessions, max_age_days } => {
            let removed = apply_retention(&workspace, RetentionPolicy { max_sessions, max_age_days })
                .context("failed to apply retention")?;
            println!("Garbage-collected {removed} session(s).");
        }
        SessionStoreCommand::List { limit } => {
            let sessions = recent_sessions(&workspace, limit);
            if sessions.is_empty() {
                println!("No sessions found under .vtcode/sessions/.");
                return Ok(());
            }
            for s in &sessions {
                println!(
                    "{:<28} turns={:<4} events={:<6} {}  {}",
                    s.session_id, s.turn_count, s.event_count, s.status, s.updated_at
                );
            }
            println!("{} session(s).", sessions.len());
        }
        SessionStoreCommand::Inspect { session } => {
            let log = open(&workspace, &session, DEFAULT_MAX_EVENTS).context("failed to open session")?;
            let manifest = log.manifest();
            println!("session_id:   {}", manifest.session_id);
            println!("status:       {}", manifest.status);
            println!("turns:        {}", manifest.turn_count);
            println!("events:       {}", manifest.event_count);
            println!("created_at:   {}", manifest.created_at);
            println!("updated_at:   {}", manifest.updated_at);
        }
        SessionStoreCommand::Facts { limit } => {
            let facts = query_facts(&workspace, limit).context("failed to query facts")?;
            if facts.is_empty() {
                println!("No grounded facts found across sessions.");
                return Ok(());
            }
            for f in &facts {
                println!("- [{}] {}", f.session_id, f.fact);
            }
            println!("{} fact(s).", facts.len());
        }
        SessionStoreCommand::Pack { session, output, verify } => {
            if let Some(pack_path) = verify {
                let pack = read_audit_pack(&pack_path)
                    .with_context(|| format!("failed to read audit pack {}", pack_path.display()))?;
                let report = verify_audit_pack(&workspace, &session, &pack).context("failed to verify audit pack")?;
                if report.verified {
                    println!("VERIFIED: {} files match the audit pack.", pack.entries.len());
                } else {
                    println!("FAILED: audit pack does not match the current session contents.");
                }
                for m in &report.mismatches {
                    println!("  MODIFIED: {m}");
                }
                for m in &report.missing {
                    println!("  MISSING:  {m}");
                }
                for u in &report.unaccounted {
                    println!("  NEW:      {u}");
                }
                if !report.verified {
                    anyhow::bail!("audit pack verification failed");
                }
                return Ok(());
            }
            let (pack, path) =
                write_audit_pack(&workspace, &session, output.as_deref()).context("failed to create audit pack")?;
            println!(
                "Wrote audit pack to {} ({} files, session status: {}, turns: {}, events: {}).",
                path.display(),
                pack.entries.len(),
                pack.status,
                pack.turn_count,
                pack.event_count
            );
        }
    }
    Ok(())
}
