#![allow(unused_imports)]

#[allow(unused_imports)]
use super::*;

pub(super) struct ResolvedSearchPath {
    pub(super) command_arg: String,
    pub(super) display_path: String,
}

pub(super) async fn build_resolved_workspace_path(
    workspace_root: &Path,
    resolved: PathBuf,
) -> Result<ResolvedSearchPath> {
    let workspace_root = tokio::fs::canonicalize(workspace_root)
        .await
        .with_context(|| {
            format!(
                "Failed to canonicalize workspace root {}",
                workspace_root.display()
            )
        })?;

    build_resolved_workspace_path_inner(&workspace_root, resolved)
}

/// Inner helper that builds a `ResolvedSearchPath` from an already-canonicalized
/// workspace root. Used by callers that have already canonicalized to avoid
/// redundant (and potentially blocking) filesystem calls.
pub(super) fn build_resolved_workspace_path_inner(
    workspace_root: &Path,
    resolved: PathBuf,
) -> Result<ResolvedSearchPath> {
    let display_path = if let Ok(relative) = resolved.strip_prefix(workspace_root) {
        if relative.as_os_str().is_empty() {
            ".".to_string()
        } else {
            relative.to_string_lossy().replace('\\', "/")
        }
    } else {
        resolved.to_string_lossy().replace('\\', "/")
    };

    let command_arg = if display_path == "." {
        ".".to_string()
    } else if Path::new(&display_path).is_relative() {
        display_path.clone()
    } else {
        resolved.to_string_lossy().to_string()
    };

    Ok(ResolvedSearchPath {
        command_arg,
        display_path,
    })
}

pub(super) async fn resolve_search_path(
    workspace_root: &Path,
    requested_path: &str,
) -> Result<ResolvedSearchPath> {
    let requested = PathBuf::from(requested_path);
    let resolved = resolve_workspace_path(workspace_root, requested.as_path())
        .or_else(|original_error| {
            let Some(remapped) =
                remap_legacy_crates_search_path(workspace_root, requested.as_path())
            else {
                return Err(original_error);
            };
            resolve_workspace_path(workspace_root, remapped.as_path()).with_context(|| {
                format!("Failed to resolve structural search path: {requested_path}")
            })
        })
        .with_context(|| format!("Failed to resolve structural search path: {requested_path}"))?;

    build_resolved_workspace_path(workspace_root, resolved).await
}

pub(super) fn remap_legacy_crates_search_path(
    workspace_root: &Path,
    requested_path: &Path,
) -> Option<PathBuf> {
    let relative = if requested_path.is_absolute() {
        requested_path.strip_prefix(workspace_root).ok()?
    } else {
        requested_path
    };

    let mut components = relative.components();
    match components.next()? {
        Component::Normal(component) if component == "crates" => {}
        _ => return None,
    }

    let remapped: PathBuf = components.collect();
    if remapped.as_os_str().is_empty() {
        return None;
    }

    workspace_root.join(&remapped).exists().then_some(remapped)
}

pub(super) async fn resolve_config_path(
    workspace_root: &Path,
    requested_path: &str,
    require_exists: bool,
) -> Result<ResolvedSearchPath> {
    let candidate = if Path::new(requested_path).is_absolute() {
        PathBuf::from(requested_path)
    } else {
        workspace_root.join(requested_path)
    };
    let normalized = normalize_path(&candidate);
    let resolved = canonicalize_allow_missing(&normalized)
        .await
        .with_context(|| format!("Failed to resolve structural config path: {requested_path}"))?;
    let workspace_root = tokio::fs::canonicalize(workspace_root)
        .await
        .with_context(|| {
            format!(
                "Failed to canonicalize workspace root {}",
                workspace_root.display()
            )
        })?;
    if !resolved.starts_with(&workspace_root) {
        bail!(
            "Path {} escapes workspace root {}",
            resolved.display(),
            workspace_root.display()
        );
    }

    if require_exists && !resolved.is_file() {
        let is_default = requested_path == DEFAULT_AST_GREP_CONFIG_PATH;
        let discovered = if is_default {
            discover_project_config(&workspace_root).await
        } else {
            None
        };
        bail!(
            "{}",
            format_missing_config_error(
                requested_path,
                is_default,
                &resolved,
                discovered.as_deref()
            )
        );
    }

    build_resolved_workspace_path_inner(&workspace_root, resolved)
}

/// Walk up from `start` looking for `sgconfig.yml` in ancestor directories.
pub(super) async fn discover_project_config(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start.to_path_buf());
    while let Some(dir) = current {
        let candidate = dir.join(DEFAULT_AST_GREP_CONFIG_PATH);
        if candidate.is_file() {
            return Some(candidate);
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    None
}

pub(super) fn format_missing_config_error(
    requested: &str,
    is_default: bool,
    resolved: &Path,
    discovered: Option<&Path>,
) -> String {
    let mut message = if is_default {
        format!(
            "ast-grep project config `{}` not found at {}. \
             `workflow=\"scan\"` and `workflow=\"test\"` require a project config file. \
             Create `{}` with at least:\n\n  ruleDirs:\n    - rules\n\n\
             Then place rule YAML files in the `rules/` directory. \
             Or scaffold a full project with `ast-grep new project`. \
             For config authoring, load the bundled `ast-grep` skill.",
            requested,
            resolved.display(),
            requested,
        )
    } else {
        format!(
            "ast-grep project config not found at `{}` (resolved to {}). \
             Verify the `config_path` is correct and the file exists. \
             For config authoring, load the bundled `ast-grep` skill.",
            requested,
            resolved.display(),
        )
    };

    if let Some(found) = discovered {
        message.push_str(&format!(
            "\n\nNote: found `{}` at {}. \
             Set `config_path` to that path to use it, or create a local `{}` in the workspace root.",
            DEFAULT_AST_GREP_CONFIG_PATH,
            found.display(),
            DEFAULT_AST_GREP_CONFIG_PATH,
        ));
    }

    message
}

/// Best-effort extraction of `ruleDirs` entries from a sgconfig.yml file.
/// Returns relative directory paths found under the `ruleDirs:` key.
pub(super) async fn extract_rule_dirs(config_path: &Path) -> Vec<String> {
    extract_string_list_from_yaml(config_path, "ruleDirs").await
}
