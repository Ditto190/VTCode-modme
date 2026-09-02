//! Tool outcome handlers for the agent turn loop.
//!
//! This module contains the functions for handling tool execution outcomes:
//! - Permission checking (prepare)
//! - Execution with caching
//! - Success/failure/timeout/cancelled handling

mod apply;
mod dispatch;
pub(crate) mod error_handling;
mod execution_result;
pub(crate) mod handlers;
pub(crate) mod helpers;
pub(crate) mod read_extent;
mod response_content;
mod subagent_memory;

pub(crate) use apply::apply_turn_outcome;
pub(crate) use dispatch::handle_tool_calls;
pub(crate) use execution_result::{
    ToolFailureDiagnosis, bounded_diagnostic_field, bounded_error_evidence, bounded_output_evidence,
    deterministic_error_diagnosis, deterministic_output_diagnosis, escape_untrusted_evidence, render_diagnosis,
};
pub(crate) use handlers::ToolOutcomeContext;
