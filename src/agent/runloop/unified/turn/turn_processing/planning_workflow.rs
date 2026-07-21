//! Agent Legibility:
//! - Entrypoint: `maybe_force_planning_workflow_interview` is the sole orchestrator; it decides via `gating` whether an interview is ready/needed and, if so, injects a static clarifying-question interview via `interview_forcing`.
//! - Common changes:
//!   - Pure readiness/need-state predicates (no I/O) live in `gating.rs` — extend those for new interview-skip conditions.
//!   - Static fallback question shaping lives in `interview_payload.rs` (`build_fallback_question`); plan draft / open-decision detection helpers live in `interview_context.rs`.
//! - Constraints: This file is intentionally kept to orchestration + shared constants only. Put new logic in `gating.rs` (pure predicates) or `interview_payload.rs` (question shaping) — do not grow this root file's function bodies.
//! - Verify: `cargo check -p vtcode && cargo test -p vtcode --bin vtcode inline_events::tests`

use crate::agent::runloop::unified::planning_workflow_state::PlanningWorkflowSessionState;

#[path = "planning_workflow/gating.rs"]
mod gating;
#[path = "planning_workflow/interview_context.rs"]
mod interview_context;
#[path = "planning_workflow/interview_forcing.rs"]
mod interview_forcing;
#[path = "planning_workflow/interview_payload.rs"]
mod interview_payload;

use crate::agent::runloop::unified::turn::context::TurnProcessingResult;
pub(crate) use gating::planning_workflow_interview_ready;
use interview_forcing::{filter_interview_tool_calls, maybe_append_planning_workflow_reminder};

#[cfg(test)]
use super::response_processing::prepare_tool_calls;

#[cfg(test)]
use vtcode_core::llm::provider as uni;

const MIN_PLANNING_WORKFLOW_TURNS_BEFORE_INTERVIEW: usize = 1;
const PLANNING_WORKFLOW_REMINDER: &str = vtcode_core::prompts::system::PLANNING_WORKFLOW_IMPLEMENT_REMINDER;

pub(crate) fn maybe_force_planning_workflow_interview(
    processing_result: TurnProcessingResult,
    response_text: Option<&str>,
    _session_stats: &mut crate::agent::runloop::unified::state::SessionStats,
    plan_session: &mut PlanningWorkflowSessionState,
    _conversation_len: usize,
) -> TurnProcessingResult {
    let filter_outcome = filter_interview_tool_calls(processing_result, plan_session, false, false, false);
    let processing_result = filter_outcome.processing_result;

    let response_has_plan = response_text.map(|text| text.contains("<proposed_plan>")).unwrap_or(false);
    if response_has_plan {
        return maybe_append_planning_workflow_reminder(processing_result);
    }

    processing_result
}

#[cfg(test)]
mod tests;
