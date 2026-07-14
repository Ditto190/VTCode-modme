//! Pure decision predicates for whether the planning-workflow interview
//! should be offered, requested from the model, or forced this turn.
//!
//! Interface contract: every function here is a pure read of
//! [`SessionStats`] / [`PlanningWorkflowSessionState`] / response text — no
//! LLM calls, no tool execution, no mutation of `plan_session`. This keeps
//! the readiness/need decision independently testable and decoupled from
//! synthesis (`super::synthesis`) and orchestration (`super::
//! maybe_force_planning_workflow_interview`), which are the only callers
//! outside this module.
use vtcode_core::config::constants::tools;

use super::MIN_PLANNING_WORKFLOW_TURNS_BEFORE_INTERVIEW;
use super::interview_context::has_open_decision_markers;
use super::interview_forcing::turn_result_has_interview_tool_call;
use crate::agent::runloop::unified::planning_workflow_state::PlanningWorkflowSessionState;
use crate::agent::runloop::unified::state::SessionStats;
use crate::agent::runloop::unified::turn::context::TurnProcessingResult;

#[derive(Debug, Clone, Copy)]
pub(super) struct InterviewNeedState {
    pub(super) response_has_plan: bool,
    pub(super) needs_interview: bool,
}

fn has_discovery_tool(session_stats: &SessionStats) -> bool {
    [
        tools::READ_FILE,
        tools::LIST_FILES,
        tools::GREP_FILE,
        tools::UNIFIED_SEARCH,
        tools::UNIFIED_EXEC,
        tools::CODE_SEARCH,
    ]
    .iter()
    .any(|tool| session_stats.has_tool(tool))
}

/// Whether the interview may be shown at all this turn (independent of
/// whether it is *needed* — see [`interview_need_state`]).
pub(crate) fn planning_workflow_interview_ready(
    session_stats: &SessionStats,
    plan_session: &PlanningWorkflowSessionState,
) -> bool {
    // Do NOT allow interview when budget is exhausted — no further LLM calls
    // are possible and re-forcing would loop forever. The same applies when
    // post-tool recovery is exhausted: the planning context is saturated and
    // re-forcing the interview would re-research and loop forever. Likewise,
    // once `request_user_input` has been permanently denied (policy/capability
    // failure, not a user cancellation), re-forcing it would just repeat the
    // same denial every turn (checkpoint turn_655/turn_660).
    if plan_session.is_budget_exhausted()
        || plan_session.is_recovery_exhausted()
        || plan_session.is_interview_denied()
    {
        return false;
    }
    has_discovery_tool(session_stats)
        && plan_session.turns() >= MIN_PLANNING_WORKFLOW_TURNS_BEFORE_INTERVIEW
}

pub(crate) fn should_attempt_dynamic_interview_generation(
    processing_result: &TurnProcessingResult,
    response_text: Option<&str>,
    session_stats: &SessionStats,
    plan_session: &PlanningWorkflowSessionState,
) -> bool {
    // Do NOT attempt interview generation when budget is exhausted — no further
    // LLM calls are possible and the interview would loop forever. The same
    // applies when post-tool recovery is exhausted (saturated planning context)
    // or the interview has been permanently denied by policy.
    if plan_session.is_budget_exhausted()
        || plan_session.is_recovery_exhausted()
        || plan_session.is_interview_denied()
    {
        return false;
    }
    let response_has_plan = response_text
        .map(|text| text.contains("<proposed_plan>"))
        .unwrap_or(false);
    if !planning_workflow_interview_ready(session_stats, plan_session) && !response_has_plan {
        return false;
    }

    if turn_result_has_interview_tool_call(processing_result) {
        return false;
    }

    let need_state = interview_need_state(response_text, plan_session);

    if need_state.response_has_plan {
        return need_state.needs_interview;
    }

    if plan_session.interview_pending() {
        return need_state.needs_interview;
    }

    need_state.needs_interview
}

/// Whether the planning session still needs an interview cycle, and whether
/// the response already contains a `<proposed_plan>` block.
pub(super) fn interview_need_state(
    response_text: Option<&str>,
    plan_session: &PlanningWorkflowSessionState,
) -> InterviewNeedState {
    let response_has_plan = response_text
        .map(|text| text.contains("<proposed_plan>"))
        .unwrap_or(false);
    let has_open_decisions = response_text
        .map(has_open_decision_markers)
        .unwrap_or(false);
    let has_completed_interview = plan_session.interview_cycles_completed() > 0;
    let interview_cancelled = plan_session.last_interview_cancelled();

    InterviewNeedState {
        response_has_plan,
        needs_interview: !has_completed_interview || interview_cancelled || has_open_decisions,
    }
}
