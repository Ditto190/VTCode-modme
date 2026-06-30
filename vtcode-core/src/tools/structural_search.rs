mod command;
mod constants;
mod fragment_hints;
mod inspect;
mod output_types;
mod parsing;
mod paths;
mod request;
mod result;
mod rewrite;
mod workflows;
mod yaml;

#[cfg(test)]
mod tests;

// Re-export submodule items at the module root so consumers and tests can reach
// them as `crate::tools::structural_search::Item` / `super::Item`. The submodules
// define these as `pub(super)`; widening to `pub(crate)` preserves the
// pre-split public surface.
#[allow(unused_imports)]
pub(crate) use command::*;
#[allow(unused_imports)]
pub(crate) use constants::*;
#[allow(unused_imports)]
pub(crate) use fragment_hints::*;
#[allow(unused_imports)]
pub(crate) use inspect::*;
#[allow(unused_imports)]
pub(crate) use output_types::*;
#[allow(unused_imports)]
pub(crate) use parsing::*;
#[allow(unused_imports)]
pub(crate) use paths::*;
#[allow(unused_imports)]
pub(crate) use request::*;
#[allow(unused_imports)]
pub(crate) use result::*;
#[allow(unused_imports)]
pub(crate) use rewrite::*;
#[allow(unused_imports)]
pub(crate) use workflows::*;
#[allow(unused_imports)]
pub(crate) use yaml::*;

// Shared external imports re-exported into the module namespace so each
// submodule only needs `use super::*;` rather than repeating this list.
#[allow(unused_imports)]
pub(super) use anyhow::{Context, Result, anyhow, bail};
#[allow(unused_imports)]
pub(super) use {
    crate::tools::ast_grep_binary::AST_GREP_INSTALL_COMMAND,
    crate::tools::ast_grep_installer::AstGrepStatus,
    crate::tools::ast_grep_language::AstGrepLanguage,
    crate::tools::error_helpers::deserialize_tool_args,
    crate::tools::tree_sitter_runtime::parse_source,
    crate::utils::path::{canonicalize_allow_missing, normalize_path, resolve_workspace_path},
    once_cell::sync::Lazy,
    regex::Regex,
    serde::{Deserialize, Deserializer},
    serde_json::{Map, Value, json},
    std::collections::BTreeMap,
    std::fmt,
    std::path::{Component, Path, PathBuf},
    tokio::fs as afs,
    tokio::process::Command,
};

/// Execute a structural (`ast-grep`) search/rewrite request.
///
/// Dispatches to the per-workflow handler in [`workflows`].
pub async fn execute_structural_search(workspace_root: &Path, args: Value) -> Result<Value> {
    let request = StructuralSearchRequest::from_args(&args)?;
    // Pure-Rust workflows that don't need the ast-grep binary.
    if request.workflow == StructuralWorkflow::Inspect {
        return execute_structural_inspect(workspace_root, &request).await;
    }
    if request.workflow == StructuralWorkflow::Rules {
        return execute_structural_rules(workspace_root, &request).await;
    }
    let ast_grep = AstGrepStatus::resolve_or_install().await.map_err(|reason| {
        let base = format!("Structural search requires ast-grep (`sg`). {reason}");
        match request.workflow {
            StructuralWorkflow::Query | StructuralWorkflow::Count => {
                anyhow!(
                    "{base} Alternatively, use `action=\"grep\"` for text-based search (regex/literal) which does not require ast-grep."
                )
            }
            _ => {
                anyhow!(
                    "{base} The `{}` workflow requires AST structure and cannot be replaced by text search.",
                    request.workflow.as_str()
                )
            }
        }
    })?;
    match request.workflow {
        StructuralWorkflow::Query => {
            execute_structural_query(workspace_root, &request, &ast_grep).await
        }
        StructuralWorkflow::Scan => {
            execute_structural_scan(workspace_root, &request, &ast_grep).await
        }
        StructuralWorkflow::Test => {
            execute_structural_test(workspace_root, &request, &ast_grep).await
        }
        StructuralWorkflow::Inspect => {
            bail!("Inspect workflow should be handled before this point")
        }
        StructuralWorkflow::Rewrite => {
            execute_structural_rewrite(workspace_root, &request, &ast_grep).await
        }
        StructuralWorkflow::Count => {
            execute_structural_count(workspace_root, &request, &ast_grep).await
        }
        StructuralWorkflow::Rules => {
            bail!("Rules workflow should be handled before this point")
        }
        StructuralWorkflow::New => {
            execute_structural_new(workspace_root, &request, &ast_grep).await
        }
        StructuralWorkflow::Apply => {
            execute_structural_apply(workspace_root, &request, &ast_grep).await
        }
    }
}
