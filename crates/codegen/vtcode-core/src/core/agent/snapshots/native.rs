use super::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone)]
struct Active {
    workspace: PathBuf,
    storage: PathBuf,
    record: PathBuf,
    engine: String,
    watch: String,
}
static ACTIVE: OnceLock<Mutex<HashMap<String, Arc<Mutex<Active>>>>> = OnceLock::new();
fn active_map() -> &'static Mutex<HashMap<String, Arc<Mutex<Active>>>> {
    ACTIVE.get_or_init(Mutex::default)
}

/// Holds exclusive workspace access until the complete agent turn has settled.
pub struct PromptCheckpointLease {
    key: String,
    _lock: fs::File,
}
impl Drop for PromptCheckpointLease {
    fn drop(&mut self) {
        if let Ok(mut active) = active_map().lock() {
            active.remove(&self.key);
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Recovery {
    #[serde(default)]
    policy: String,
    snapshot: String,
    active: Vec<usize>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Navigation {
    active: Vec<usize>,
    redo: Vec<Recovery>,
    pending: Option<Recovery>,
}

fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    use std::io::Write;
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    fs::rename(temp, path)?;
    Ok(())
}
fn file_records(manifest: &filesnap::Manifest, workspace: &Path, engine: &str) -> Result<Vec<FileSnapshot>> {
    manifest
        .entries
        .keys()
        .chain(manifest.absent.iter())
        .map(|path| {
            let relative = Path::new(path)
                .strip_prefix(workspace)
                .context("Checkpoint escaped the workspace")?;
            Ok(FileSnapshot {
                path: relative.to_string_lossy().replace('\\', "/"),
                deleted: manifest.absent.contains(path),
                encoding: Some(FileEncoding::Filesnap),
                data: Some(engine.to_owned()),
            })
        })
        .collect()
}

/// Capture final local write arguments after host hook rewriting and before tool execution.
pub async fn declare_prompt_edit(session: String, name: String, args: serde_json::Value) -> Result<()> {
    let active = active_map()
        .lock()
        .map_err(|_| anyhow::anyhow!("Checkpoint lock poisoned"))?
        .get(&session)
        .cloned();
    let Some(active) = active else {
        return Ok(());
    };
    let operation = args.get("action").and_then(serde_json::Value::as_str).unwrap_or(&name);
    if !["write", "edit", "patch", "delete", "remove", "create", "move", "rename"]
        .iter()
        .any(|verb| operation.contains(verb))
    {
        return Ok(());
    }
    tokio::task::spawn_blocking(move || -> Result<()> {
        let active = active.lock().map_err(|_| anyhow::anyhow!("Checkpoint lock poisoned"))?;
        let mut paths = BTreeSet::new();
        for key in [
            "path",
            "file_path",
            "destination",
            "destination_path",
            "new_path",
            "source",
        ] {
            if let Some(path) = args.get(key).and_then(serde_json::Value::as_str) {
                paths.insert(PathBuf::from(path));
            }
        }
        for key in ["patch", "input", "patch_text"] {
            if let Some(patch) = args.get(key).and_then(serde_json::Value::as_str) {
                for line in patch.lines() {
                    for prefix in [
                        "*** Add File: ",
                        "*** Update File: ",
                        "*** Delete File: ",
                        "*** Move to: ",
                    ] {
                        if let Some(path) = line.strip_prefix(prefix) {
                            paths.insert(PathBuf::from(path));
                        }
                    }
                }
            }
        }
        let store = filesnap::WorkspaceStore::open(&active.storage, &active.workspace)?;
        for path in paths {
            let relative = if path.is_absolute() {
                path.strip_prefix(&active.workspace)
                    .context("Edit outside checkpoint workspace")?
                    .to_path_buf()
            } else {
                path
            };
            let path = SnapshotManager::checked_file_path(&active.workspace, &active.storage, &relative)?;
            let image = match fs::read(&path) {
                Ok(bytes) => filesnap::PreEditImage::Existed(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => filesnap::PreEditImage::DidNotExist,
                Err(error) => return Err(error.into()),
            };
            store.declare_paths(&active.watch, &active.engine, &[path.clone()])?;
            filesnap::declare_edits(
                &store,
                &active.engine,
                &active.engine,
                &filesnap::TurnScope::at(&active.workspace),
                vec![(path, image)],
            )?;
        }
        let target = store.target_for_turn(&active.engine)?.context("Missing active checkpoint")?;
        let manifest = store.manifest(target.manifest_id())?;
        let mut stored: StoredSnapshot = serde_json::from_slice(&fs::read(&active.record)?)?;
        stored.files = file_records(&manifest, &active.workspace, &active.engine)?;
        stored.metadata.file_count = stored.files.len();
        atomic_json(&active.record, &stored)
    })
    .await?
}

impl SnapshotManager {
    fn navigation_path(&self, session: &str) -> Result<PathBuf> {
        anyhow::ensure!(session.len() <= 80 && !session.is_empty(), "Invalid session ID");
        let key: String = session.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
        Ok(self.storage_dir.join(format!("branch_{key}.json")))
    }
    fn navigation(&self, session: &str) -> Result<Navigation> {
        match fs::read(self.navigation_path(session)?) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Navigation::default()),
            Err(error) => Err(error.into()),
        }
    }
    /// Save the exact conversation prefix and workspace before sending a prompt.
    pub async fn begin_prompt(
        &self,
        turn: usize,
        session: &str,
        prompt: &str,
        conversation: &[SessionMessage],
    ) -> Result<PromptCheckpointLease> {
        let mut state = self.navigation(session)?;
        anyhow::ensure!(state.pending.is_none(), "Interrupted rewind; run /rewind-recover before continuing");
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.storage_dir.join("rewind.lock"))?;
        lock.try_lock().context("Another turn or rewind is using this workspace")?;
        let workspace = self.canonical_workspace.clone();
        let storage = self.storage_dir.clone();
        let engine = format!("vt-{}", uuid::Uuid::new_v4());
        let watch = format!("vt-watch-{session}");
        let active = Active {
            workspace: workspace.clone(),
            storage: storage.clone(),
            record: self.snapshot_path(turn),
            engine: engine.clone(),
            watch: watch.clone(),
        };
        let files = tokio::task::spawn_blocking(move || -> Result<Vec<FileSnapshot>> {
            let store = filesnap::WorkspaceStore::open(&storage, &workspace)?;
            store.note_turn(&watch, &engine)?;
            let watched: Vec<_> = store
                .declared_paths(&watch, filesnap::DeclaredWindow::default())?
                .into_iter()
                .collect();
            store.declare_paths(&engine, &engine, &watched)?;
            let checkpoint = filesnap::capture_turn(&store, &engine, &engine, &filesnap::TurnScope::at(&workspace))?;
            anyhow::ensure!(checkpoint.stats.dropped == 0, "Checkpoint skipped paths; cannot safely start turn");
            let files = file_records(&checkpoint.manifest, &workspace, &engine)?;
            for file in &files {
                SnapshotManager::checked_file_path(&workspace, &storage, Path::new(&file.path))?;
            }
            Ok(files)
        })
        .await??;
        let metadata = SnapshotMetadata {
            id: format!("turn_{turn}"),
            turn_number: turn,
            created_at: Self::current_timestamp()?,
            description: Self::truncate_description(prompt),
            message_count: conversation.len(),
            file_count: files.len(),
            touched_files: vec![],
            prompt_text: Some(prompt.into()),
            prompt_message_index: None,
            session_id: Some(session.into()),
            runtime_turn_id: None,
            session_turn_number: Some(turn),
            turn_diagnostics: None,
        };
        atomic_json(
            &active.record,
            &StoredSnapshot {
                metadata,
                conversation: conversation.to_vec(),
                files,
                schema_version: Some(SNAPSHOT_SCHEMA_VERSION),
            },
        )?;
        state.active.push(turn);
        state.redo.clear();
        atomic_json(&self.navigation_path(session)?, &state)?;
        active_map()
            .lock()
            .map_err(|_| anyhow::anyhow!("Checkpoint lock poisoned"))?
            .insert(session.into(), Arc::new(Mutex::new(active)));
        Ok(PromptCheckpointLease { key: session.into(), _lock: lock })
    }
    /// Enumerate only checkpoints on this session's current conversation branch.
    pub async fn rewind_points(&self, session: &str) -> Result<Vec<SnapshotMetadata>> {
        let state = self.navigation(session)?;
        let mut points = Vec::new();
        for turn in state.active.iter().rev() {
            if let Some(stored) = self.load_snapshot(*turn).await? {
                points.push(stored.metadata);
            }
        }
        Ok(points)
    }
    /// Restore files and return their matching conversation, with persistent multi-level redo.
    pub async fn navigate_prompt(
        &self,
        turn: Option<usize>,
        scope: RevertScope,
        session: &str,
        conversation: &[SessionMessage],
    ) -> Result<CheckpointRestore> {
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.storage_dir.join("rewind.lock"))?;
        lock.try_lock().context("Wait for the current turn to finish")?;
        let policy = match fs::read_to_string(self.canonical_workspace.join(".filesnapignore")) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.into()),
        };
        let build_ignore = |policy: &str| -> Result<filesnap::Gitignore> {
            anyhow::ensure!(policy.len() <= 1024 * 1024, "Ignore policy is too large");
            let mut builder = filesnap::GitignoreBuilder::new(&self.canonical_workspace);
            for line in policy.lines() {
                builder.add_line(None, line)?;
            }
            Ok(builder.build()?)
        };
        let ignore = build_ignore(&policy)?;
        let mut state = self.navigation(session)?;
        let load_recovery = |saved: &Recovery| -> Result<StoredSnapshot> {
            uuid::Uuid::parse_str(&saved.snapshot)?;
            Ok(serde_json::from_slice(&fs::read(
                self.storage_dir.join(format!("turn_recovery_{}.json", saved.snapshot)),
            )?)?)
        };
        if let Some(pending) = state.pending.clone() {
            anyhow::ensure!(turn.is_none(), "Interrupted rewind; run /rewind-recover");
            let restored = self
                .restore_stored_snapshot_with_ignore(
                    load_recovery(&pending)?,
                    RevertScope::Both,
                    &build_ignore(&pending.policy)?,
                )
                .await?;
            state.pending = None;
            atomic_json(&self.navigation_path(session)?, &state)?;
            return Ok(restored);
        }
        let (targets, next_active) = if let Some(turn) = turn {
            let index = state
                .active
                .iter()
                .position(|id| *id == turn)
                .context("Checkpoint is not on this branch")?;
            let mut targets = Vec::new();
            for id in state.active[index..].iter().rev() {
                targets.push(self.load_snapshot(*id).await?.context("Checkpoint expired")?);
            }
            (targets, state.active[..index].to_vec())
        } else {
            let saved = state.redo.last().context("Nothing to redo; new prompts clear redo")?;
            (vec![load_recovery(saved)?], saved.active.clone())
        };
        let destination = targets.last().context("No restore target")?;
        let mut rescue = destination.clone();
        rescue.conversation = conversation.to_vec();
        rescue.metadata.prompt_text = None;
        rescue.metadata.prompt_message_index = None;
        rescue.metadata.message_count = conversation.len();
        let paths: BTreeSet<String> = targets
            .iter()
            .flat_map(|target| target.files.iter().map(|f| f.path.clone()))
            .collect();
        let workspace = self.canonical_workspace.clone();
        let storage = self.storage_dir.clone();
        rescue.files = tokio::task::spawn_blocking(move || -> Result<Vec<FileSnapshot>> {
            let paths = paths
                .iter()
                .map(|p| Self::checked_file_path(&workspace, &storage, Path::new(p)))
                .collect::<Result<Vec<_>>>()?;
            let engine = format!("vt-{}", uuid::Uuid::new_v4());
            let store = filesnap::WorkspaceStore::open(&storage, &workspace)?;
            let checkpoint = store.checkpoint(&engine, &engine, paths)?;
            anyhow::ensure!(checkpoint.stats.dropped == 0, "Could not save recovery files; rewind cancelled");
            file_records(&checkpoint.manifest, &workspace, &engine)
        })
        .await??;
        let saved = Recovery {
            policy,
            snapshot: uuid::Uuid::new_v4().to_string(),
            active: state.active.clone(),
        };
        atomic_json(&self.storage_dir.join(format!("turn_recovery_{}.json", saved.snapshot)), &rescue)?;
        state.pending = Some(saved.clone());
        atomic_json(&self.navigation_path(session)?, &state)?;
        let destination = CheckpointRestore {
            metadata: destination.metadata.clone(),
            conversation: destination.conversation.clone(),
        };
        for target in targets {
            if let Err(error) = self.restore_stored_snapshot_with_ignore(target, scope, &ignore).await {
                self.restore_stored_snapshot_with_ignore(rescue, RevertScope::Both, &ignore)
                    .await
                    .context(format!("Rewind failed ({error}); recovery failed; run /rewind-recover"))?;
                state.pending = None;
                atomic_json(&self.navigation_path(session)?, &state)?;
                return Err(error.context("Rewind failed; original files recovered"));
            }
        }
        state.pending = None;
        state.active = next_active;
        if turn.is_some() {
            state.redo.push(saved);
        } else {
            state.redo.pop();
        }
        atomic_json(&self.navigation_path(session)?, &state)?;
        Ok(destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::provider::MessageRole;
    use tempfile::TempDir;
    #[tokio::test]
    async fn native_history_restores_prompt_prefix_binary_assets_and_nested_redo() -> Result<()> {
        let dir = TempDir::new()?;
        let manager = SnapshotManager::new(SnapshotConfig::new(dir.path().into()))?;
        let session = uuid::Uuid::new_v4().to_string();
        let asset = dir.path().join("asset.bin");
        fs::write(&asset, [0, 255, 7])?;
        let original = vec![SessionMessage::new(MessageRole::User, "original")];
        let lease = manager.begin_prompt(1, &session, "first", &original).await?;
        fs::write(&asset, "first result")?;
        drop(lease);
        let second = vec![SessionMessage::new(MessageRole::User, "second")];
        let lease = manager.begin_prompt(2, &session, "second", &second).await?;
        declare_prompt_edit(session.clone(), "write_file".into(), serde_json::json!({"path":".created"})).await?;
        fs::write(dir.path().join(".created"), "second result")?;
        drop(lease);
        let current = vec![SessionMessage::new(MessageRole::User, "current")];
        let restored = manager.navigate_prompt(Some(2), RevertScope::Both, &session, &current).await?;
        assert_eq!(restored.conversation, second);
        assert!(!dir.path().join(".created").exists());
        let restored = manager
            .navigate_prompt(Some(1), RevertScope::Both, &session, &restored.conversation)
            .await?;
        assert_eq!(restored.conversation, original);
        assert_eq!(fs::read(&asset)?, vec![0, 255, 7]);
        let restored = manager
            .navigate_prompt(None, RevertScope::Both, &session, &restored.conversation)
            .await?;
        assert_eq!(restored.conversation, second);
        let restored = manager
            .navigate_prompt(None, RevertScope::Both, &session, &restored.conversation)
            .await?;
        assert_eq!(restored.conversation, current);
        assert_eq!(fs::read_to_string(dir.path().join(".created"))?, "second result");
        assert!(
            manager
                .navigate_prompt(None, RevertScope::Both, &session, &current)
                .await
                .is_err()
        );
        // One jump across both prompts must also reverse later file creation.
        let restored = manager.navigate_prompt(Some(1), RevertScope::Both, &session, &current).await?;
        assert_eq!(restored.conversation, original);
        assert!(!dir.path().join(".created").exists());
        let resumed = SnapshotManager::new(SnapshotConfig::new(dir.path().into()))?;
        let restored = resumed.navigate_prompt(None, RevertScope::Both, &session, &original).await?;
        assert_eq!(restored.conversation, current);
        Ok(())
    }
}
