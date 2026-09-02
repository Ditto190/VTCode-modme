use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use uuid::Uuid;

use crate::utils::session_archive::VerifiedSessionArchiveIdentifier;
use crate::utils::session_debug::sanitize_debug_component;

const TASKS_DIR: &str = ".vtcode/tasks";
const CURRENT_BLOCKED_FILE: &str = "current_blocked.md";
const BLOCKERS_DIR: &str = "blockers";
const CURRENT_TASK_FILE: &str = "current_task.md";

struct BlockedHandoffPaths<'a> {
    tracker: &'a Path,
    current: &'a Path,
    archive: &'a Path,
}

/// Artifacts produced by [`write_blocked_handoff`], containing paths to the
/// current and archived handoff files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedHandoffArtifacts {
    /// Path to the current blocked handoff markdown file.
    pub current_path: PathBuf,
    /// Path to the archived blocked handoff markdown file.
    pub archive_path: PathBuf,
}

/// Resume metadata for a blocked handoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockedHandoffResume<'a> {
    /// The identifier came from a verified, persisted session archive.
    Available(&'a VerifiedSessionArchiveIdentifier),
    /// No durable archive can be advertised for this handoff.
    Unavailable(&'a str),
}

/// Write a blocked-handoff artifact when the agent hits an unrecoverable blocker.
///
/// Creates both a `current_blocked.md` file and a timestamped archive under
/// `.vtcode/tasks/blockers/`. The handoff includes the blocker summary, current
/// tracker snapshot. Resume commands are added only by
/// [`write_blocked_handoff_with_resume`] after a caller verifies an
/// archive identifier.
pub fn write_blocked_handoff(
    workspace: &Path,
    session_id: &str,
    outcome_code: &str,
    blocker_summary: &str,
    relevant_paths: &[PathBuf],
) -> Result<BlockedHandoffArtifacts> {
    write_blocked_handoff_with_resume(
        workspace,
        session_id,
        outcome_code,
        blocker_summary,
        relevant_paths,
        BlockedHandoffResume::Unavailable(
            "Resume is unavailable because this compatibility entry point has no verified session archive.",
        ),
    )
}

/// Write a blocked handoff with resume metadata supplied through the typed
/// archive-verification boundary.
pub fn write_blocked_handoff_with_resume(
    workspace: &Path,
    session_id: &str,
    outcome_code: &str,
    blocker_summary: &str,
    relevant_paths: &[PathBuf],
    resume: BlockedHandoffResume<'_>,
) -> Result<BlockedHandoffArtifacts> {
    let tasks_dir = workspace.join(TASKS_DIR);
    let blockers_dir = tasks_dir.join(BLOCKERS_DIR);
    fs::create_dir_all(&blockers_dir)
        .with_context(|| format!("failed to create blockers dir {}", blockers_dir.display()))?;

    let tracker_path = tasks_dir.join(CURRENT_TASK_FILE);
    let current_path = tasks_dir.join(CURRENT_BLOCKED_FILE);
    let timestamp = Utc::now();
    let archive_name =
        format!("{}-{}.md", sanitize_debug_component(session_id, "session"), timestamp.format("%Y%m%dT%H%M%SZ"));
    let archive_path = blockers_dir.join(archive_name);

    let markdown = render_blocked_handoff(
        workspace,
        session_id,
        outcome_code,
        blocker_summary,
        BlockedHandoffPaths {
            tracker: &tracker_path,
            current: &current_path,
            archive: &archive_path,
        },
        relevant_paths,
        timestamp.to_rfc3339(),
        resume,
    );

    fs::write(&current_path, &markdown).with_context(|| format!("failed to write {}", current_path.display()))?;
    fs::write(&archive_path, markdown).with_context(|| format!("failed to write {}", archive_path.display()))?;

    Ok(BlockedHandoffArtifacts { current_path, archive_path })
}

/// Parsed information from a blocked handoff file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedHandoffInfo {
    pub session_id: String,
    pub outcome_code: String,
    pub blocker_summary: String,
    pub created_at: Option<String>,
    pub resume_command: Option<String>,
}

/// Reads `.vtcode/tasks/current_blocked.md` if it exists, parsing the front-matter
/// and blocker summary.
pub fn read_current_blocked_handoff(workspace: &Path) -> Option<BlockedHandoffInfo> {
    let current_path = workspace.join(TASKS_DIR).join(CURRENT_BLOCKED_FILE);
    let content = fs::read_to_string(&current_path).ok()?;
    parse_blocked_handoff_content(&content)
}

fn parse_blocked_handoff_content(content: &str) -> Option<BlockedHandoffInfo> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }

    let mut session_id = None;
    let mut outcome_code = None;
    let mut created_at = None;
    let mut resume_command = None;

    let mut in_front_matter = true;
    let mut body_lines = Vec::new();

    for line in lines {
        if in_front_matter {
            let trimmed = line.trim();
            if trimmed == "---" {
                in_front_matter = false;
                continue;
            }
            if let Some((key, val)) = trimmed.split_once(':') {
                let key = key.trim();
                let val = val.trim().trim_matches('"').trim_matches('\'').trim().to_string();
                match key {
                    "session_id" => session_id = Some(val),
                    "outcome" => outcome_code = Some(val),
                    "created_at" => created_at = Some(val),
                    "resume_command" => resume_command = Some(val),
                    _ => {}
                }
            }
        } else {
            body_lines.push(line);
        }
    }

    let session_id = session_id?;
    let outcome_code = outcome_code.unwrap_or_else(|| "blocked".to_string());

    let mut blocker_summary = String::new();
    let mut in_summary = false;
    for line in body_lines {
        let trimmed = line.trim();
        if trimmed == "# Blocker Summary" {
            in_summary = true;
            continue;
        }
        if in_summary {
            if trimmed.starts_with('#') {
                break;
            }
            blocker_summary.push_str(line);
            blocker_summary.push('\n');
        }
    }
    let blocker_summary = blocker_summary.trim().to_string();

    Some(BlockedHandoffInfo {
        session_id,
        outcome_code,
        blocker_summary,
        created_at,
        resume_command,
    })
}

/// Clears `.vtcode/tasks/current_blocked.md` if it exists.
///
/// Returns `Ok(true)` if the file was deleted, or `Ok(false)` if it did not exist.
pub fn clear_current_blocked_handoff(workspace: &Path) -> Result<bool> {
    let current_path = workspace.join(TASKS_DIR).join(CURRENT_BLOCKED_FILE);
    match fs::remove_file(&current_path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err).with_context(|| format!("failed to remove {}", current_path.display())),
    }
}

fn render_blocked_handoff(
    workspace: &Path,
    session_id: &str,
    outcome_code: &str,
    blocker_summary: &str,
    paths: BlockedHandoffPaths<'_>,
    relevant_paths: &[PathBuf],
    created_at: String,
    resume: BlockedHandoffResume<'_>,
) -> String {
    let tracker_snapshot = fs::read_to_string(paths.tracker)
        .ok()
        .filter(|content| !content.trim().is_empty())
        .unwrap_or_else(|| "_No current tracker snapshot found._".to_string());

    let mut paths = vec![
        workspace.to_path_buf(),
        paths.tracker.to_path_buf(),
        paths.current.to_path_buf(),
        paths.archive.to_path_buf(),
    ];
    for path in relevant_paths {
        if !paths.iter().any(|existing| existing == path) {
            paths.push(path.clone());
        }
    }

    let relevant_paths_section = paths
        .iter()
        .map(|path| format!("- `{}`", path.display()))
        .collect::<Vec<_>>()
        .join("\n");

    let (resume_front_matter, resume_metadata, resume_actionable) = match resume {
        BlockedHandoffResume::Available(identifier) => (
            format!("resume_command: \"vtcode --resume {}\"\n", identifier.as_str()),
            format!("- Resume command: `vtcode --resume {}`\n", identifier.as_str()),
            format!("- From terminal: Run `vtcode --resume {}`\n", identifier.as_str()),
        ),
        BlockedHandoffResume::Unavailable(explanation) => {
            (String::new(), format!("- Resume unavailable: {}\n", explanation.trim()), String::new())
        }
    };

    let actionable_steps = format!(
        "## Actionable Next Steps\n\n- In this session: Type `continue` to retry with retained history, or provide alternative instructions.\n{resume_actionable}- Inspect details: Check `.vtcode/tasks/current_blocked.md`."
    );

    format!(
        "---\nsession_id: {session_id}\noutcome: {outcome_code}\ncreated_at: {created_at}\nworkspace: {}\n{resume_front_matter}---\n\n# Blocker Summary\n\n{}\n\n{}\n\n# Current Tracker Snapshot\n\n{}\n\n# Relevant Paths\n\n{}\n\n# Resume Metadata\n\n- Session ID: `{session_id}`\n- Outcome: `{outcome_code}`\n{resume_metadata}",
        workspace.display(),
        blocker_summary.trim(),
        actionable_steps,
        tracker_snapshot,
        relevant_paths_section,
    )
}

/// Artifacts produced by [`write_async_approval_blocker`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncApprovalArtifacts {
    /// Path to the async approval blocker markdown file.
    pub current_path: PathBuf,
    /// Unique token used to approve or reject this request via CLI.
    pub approval_token: String,
}

/// Write an async (deferred) approval blocker file.
///
/// Unlike [`write_blocked_handoff`] which signals a hard stop, this writes a
/// blocker that can be resolved out-of-band via CLI (`vtcode approve <token>`).
/// The blocker includes the approval question, tool details, and a unique token.
pub fn write_async_approval_blocker(
    workspace: &Path,
    session_id: &str,
    approval_question: &str,
    tool_name: &str,
    args: &serde_json::Value,
    estimated_cost: Option<f64>,
    notify_command: Option<&str>,
) -> Result<AsyncApprovalArtifacts> {
    let tasks_dir = workspace.join(TASKS_DIR);
    let blockers_dir = tasks_dir.join(BLOCKERS_DIR);
    fs::create_dir_all(&blockers_dir)
        .with_context(|| format!("failed to create blockers dir {}", blockers_dir.display()))?;

    let approval_token = Uuid::new_v4().to_string();
    let timestamp = Utc::now();
    let archive_name = format!(
        "async-{}-{}.md",
        sanitize_debug_component(session_id, "session"),
        timestamp.format("%Y%m%dT%H%M%SZ")
    );
    let current_path = blockers_dir.join(archive_name);

    let cost_line = estimated_cost.map(|c| format!("Estimated cost: ${c:.4}")).unwrap_or_default();

    let notify_line = notify_command.map(|cmd| format!("Notify command: `{cmd}`")).unwrap_or_default();

    let markdown = format!(
        "---\ntoken: {approval_token}\nsession_id: {session_id}\ntool: {tool_name}\ncreated_at: {created_at}\ntype: async_approval\n---\n\n\
         # Async Approval Request\n\n\
         ## Question\n\n{approval_question}\n\n\
         ## Tool\n- Name: `{tool_name}`\n- Arguments: ```json\n{args_json}\n```\n\
         {cost_line}\n{notify_line}\n\n\
         ## How to Approve\n\n\
         ```\nvtcode approve {approval_token}\nvtcode reject {approval_token}\nvtcode approve list\n```\n",
        created_at = timestamp.to_rfc3339(),
        args_json = serde_json::to_string_pretty(args).unwrap_or_else(|_| args.to_string()),
    );

    fs::write(&current_path, &markdown)
        .with_context(|| format!("failed to write async blocker {}", current_path.display()))?;

    Ok(AsyncApprovalArtifacts { current_path, approval_token })
}

#[cfg(test)]
mod tests {
    use crate::utils::session_archive::VerifiedSessionArchiveIdentifier;

    use super::*;

    #[test]
    fn writes_current_and_archived_blocked_handoffs() {
        let temp = tempfile::tempdir().expect("temp dir");
        let tasks_dir = temp.path().join(".vtcode/tasks");
        fs::create_dir_all(&tasks_dir).expect("tasks dir");
        fs::write(tasks_dir.join("current_task.md"), "# Current Task\n\n- [ ] investigate blocker\n").expect("tracker");

        let artifacts = write_blocked_handoff(
            temp.path(),
            "session-123",
            "loop_detected",
            "Execution stalled on a loop.",
            &[temp.path().join("src/lib.rs")],
        )
        .expect("write handoff");

        let current = fs::read_to_string(&artifacts.current_path).expect("current handoff");
        let archive = fs::read_to_string(&artifacts.archive_path).expect("archive handoff");

        assert_eq!(current, archive);
        assert!(current.contains("session_id: session-123"));
        assert!(current.contains("# Blocker Summary"));
        assert!(current.contains("Execution stalled on a loop."));
        assert!(current.contains("# Current Task"));
        assert!(!current.contains("resume_command:"));
        assert!(!current.contains("vtcode --resume"));
        assert!(current.contains("Resume is unavailable"));
        assert!(current.contains("src/lib.rs"));
    }

    #[test]
    fn writes_blocked_handoff_without_resume_when_archive_is_unavailable() {
        let temp = tempfile::tempdir().expect("temp dir");

        let artifacts = write_blocked_handoff_with_resume(
            temp.path(),
            "runtime-session",
            "blocked",
            "History persistence is disabled.",
            &[temp.path().join("src/lib.rs")],
            BlockedHandoffResume::Unavailable("Resume is unavailable because the session archive was not persisted."),
        )
        .expect("write handoff");

        let current = fs::read_to_string(&artifacts.current_path).expect("current handoff");
        assert!(!current.contains("resume_command:"));
        assert!(!current.contains("vtcode --resume"));
        assert!(current.contains("Resume is unavailable because the session archive was not persisted."));
    }

    #[test]
    fn uses_verified_archive_identifier_for_resume_command() {
        let temp = tempfile::tempdir().expect("temp dir");

        let verified_identifier = VerifiedSessionArchiveIdentifier("session-archive-id".to_owned());
        let artifacts = write_blocked_handoff_with_resume(
            temp.path(),
            "runtime-session",
            "blocked",
            "Execution stalled on a loop.",
            &[],
            BlockedHandoffResume::Available(&verified_identifier),
        )
        .expect("write handoff");

        let current = fs::read_to_string(&artifacts.current_path).expect("current handoff");
        assert!(current.contains("vtcode --resume session-archive-id"));
        assert!(!current.contains("vtcode --resume runtime-session"));
    }

    #[test]
    fn write_async_approval_blocker_creates_file_with_token() {
        let temp = tempfile::tempdir().expect("temp dir");
        let tasks_dir = temp.path().join(".vtcode/tasks");
        fs::create_dir_all(&tasks_dir).expect("tasks dir");

        let artifacts = write_async_approval_blocker(
            temp.path(),
            "session-456",
            "Push 50 commits to main?",
            "git_push",
            &serde_json::json!({"force": true, "branch": "main"}),
            Some(0.50),
            Some("/usr/local/bin/notify"),
        )
        .expect("write async blocker");

        assert!(!artifacts.approval_token.is_empty());
        assert!(artifacts.current_path.exists());

        let content = fs::read_to_string(&artifacts.current_path).expect("read blocker");
        assert!(content.contains("Push 50 commits to main?"));
        assert!(content.contains("git_push"));
        assert!(content.contains("Estimated cost: $0.50"));
        assert!(content.contains("vtcode approve"));
        assert!(content.contains(&artifacts.approval_token));
    }

    #[test]
    fn write_async_approval_blocker_handles_minimal_input() {
        let temp = tempfile::tempdir().expect("temp dir");
        let tasks_dir = temp.path().join(".vtcode/tasks");
        fs::create_dir_all(&tasks_dir).expect("tasks dir");

        let artifacts = write_async_approval_blocker(
            temp.path(),
            "session-789",
            "Delete the file?",
            "delete_file",
            &serde_json::json!({"path": "/tmp/x"}),
            None,
            None,
        )
        .expect("write async blocker");

        assert!(!artifacts.approval_token.is_empty());
        assert!(artifacts.current_path.exists());

        let content = fs::read_to_string(&artifacts.current_path).expect("read blocker");
        assert!(content.contains("Delete the file?"));
        assert!(content.contains("delete_file"));
        // No cost or notify section
        assert!(!content.contains("Estimated cost:"));
        assert!(!content.contains("Notify command:"));
    }

    #[test]
    fn test_read_and_clear_current_blocked_handoff() {
        let temp = tempfile::tempdir().expect("temp dir");

        // When file does not exist
        assert_eq!(read_current_blocked_handoff(temp.path()), None);
        assert!(!clear_current_blocked_handoff(temp.path()).unwrap());

        // Write a blocked handoff
        let verified_identifier = VerifiedSessionArchiveIdentifier("session-archive-id".to_owned());
        let _artifacts = write_blocked_handoff_with_resume(
            temp.path(),
            "test-session-123",
            "blocked",
            "Tool call failed repeatedly with permission errors.",
            &[],
            BlockedHandoffResume::Available(&verified_identifier),
        )
        .expect("write handoff");

        // Read it back
        let info = read_current_blocked_handoff(temp.path()).expect("read info");
        assert_eq!(info.session_id, "test-session-123");
        assert_eq!(info.outcome_code, "blocked");
        assert_eq!(info.blocker_summary, "Tool call failed repeatedly with permission errors.");
        assert!(info.created_at.is_some());
        assert_eq!(info.resume_command.as_deref(), Some("vtcode --resume session-archive-id"));

        // Clear it
        assert!(clear_current_blocked_handoff(temp.path()).unwrap());
        // Should no longer exist
        assert_eq!(read_current_blocked_handoff(temp.path()), None);
        assert!(!clear_current_blocked_handoff(temp.path()).unwrap());
    }
}
