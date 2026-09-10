use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::Notify;
use vtcode_core::core::interfaces::ui::UiSession;
use vtcode_core::tools::ToolInvocationId;
use vtcode_ui::tui::app::InlineHandle;

use crate::agent::runloop::unified::inline_events::harness::{HarnessEventEmitter, harness_event};
use crate::agent::runloop::unified::run_loop_context::{
    BudgetExhaustedMetrics, HarnessTurnState, SESSION_LIMIT_AUTO_GRANT_INCREMENT, budget_kind,
    emit_budget_exhausted_metric,
};
use crate::agent::runloop::unified::state::CtrlCState;
use crate::agent::runloop::unified::tool_call_safety::{SafetyError, ToolCallSafetyValidator};
use crate::agent::runloop::unified::tool_routing::prompt_session_limit_increase;

pub(crate) enum SafetyValidationFailure {
    SessionLimitNotIncreased,
    SessionLimitPromptFailed(anyhow::Error),
    /// The safety gateway requires human approval before this tool call can
    /// proceed.  The inner string is the justification (risk description)
    /// that should be forwarded to the HITL permission prompt.
    NeedsApproval(String),
    Validation(SafetyError),
}

/// Maximum number of session limit increase prompts before giving up.
/// This prevents an infinite loop if the user keeps approving increases.
const MAX_LIMIT_INCREASE_PROMPTS: u32 = 5;

#[allow(
    clippy::too_many_arguments,
    reason = "The validation boundary carries the UI, safety, harness, and agent context needed for a grant retry."
)]
pub(crate) async fn validate_tool_call_with_limit_prompt<S: UiSession + ?Sized>(
    safety_validator: &ToolCallSafetyValidator,
    handle: &InlineHandle,
    session: &mut S,
    ctrl_c_state: &Arc<CtrlCState>,
    ctrl_c_notify: &Arc<Notify>,
    tool_name: &str,
    args: &Value,
    invocation_id: ToolInvocationId,
    mut harness_state: Option<&mut HarnessTurnState>,
    harness_emitter: Option<&HarnessEventEmitter>,
    agent_name: Option<&str>,
    auto_grant: bool,
    traj: &vtcode_core::core::trajectory::TrajectoryLogger,
    planning_active: bool,
) -> Result<(), SafetyValidationFailure> {
    let mut limit_increase_attempts = 0u32;
    loop {
        match safety_validator
            .validate_call_with_invocation_id(tool_name, args, invocation_id)
            .await
        {
            Ok(()) => return Ok(()),
            Err(SafetyError::SessionLimitReached { max }) => {
                limit_increase_attempts += 1;
                if limit_increase_attempts > MAX_LIMIT_INCREASE_PROMPTS {
                    tracing::warn!(
                        tool = %tool_name,
                        attempts = limit_increase_attempts,
                        "Session limit increase prompts exhausted; refusing to prompt further"
                    );
                    return Err(SafetyValidationFailure::SessionLimitNotIncreased);
                }
                if auto_grant {
                    let granted = safety_validator.claim_session_auto_grant(SESSION_LIMIT_AUTO_GRANT_INCREMENT);
                    if granted == 0 {
                        tracing::warn!(
                            tool = %tool_name,
                            attempts = limit_increase_attempts,
                            "Session auto-grant headroom exhausted; refusing further automatic increases"
                        );
                        emit_budget_exhausted_metric(
                            traj,
                            BudgetExhaustedMetrics {
                                budget: budget_kind::SESSION_CALLS,
                                used: safety_validator.session_count(),
                                max: safety_validator.max_per_session(),
                                step_count: None,
                                planning_active,
                                tool_calls: harness_state.as_deref().map_or(0, |state| state.tool_calls),
                            },
                        );
                        return Err(SafetyValidationFailure::SessionLimitNotIncreased);
                    }
                    record_session_limit_grant(
                        safety_validator,
                        harness_state.as_deref_mut(),
                        harness_emitter,
                        agent_name,
                        tool_name,
                        granted,
                        limit_increase_attempts,
                        true,
                    );
                    continue;
                }
                match prompt_session_limit_increase(handle, session, ctrl_c_state, ctrl_c_notify, max, agent_name).await
                {
                    Ok(Some(increment)) => {
                        record_session_limit_grant(
                            safety_validator,
                            harness_state.as_deref_mut(),
                            harness_emitter,
                            agent_name,
                            tool_name,
                            increment,
                            limit_increase_attempts,
                            false,
                        );
                    }
                    Ok(None) => {
                        return Err(SafetyValidationFailure::SessionLimitNotIncreased);
                    }
                    Err(error) => {
                        return Err(SafetyValidationFailure::SessionLimitPromptFailed(error));
                    }
                }
            }
            Err(SafetyError::NeedsApproval(justification)) => {
                return Err(SafetyValidationFailure::NeedsApproval(justification));
            }
            Err(error) => return Err(SafetyValidationFailure::Validation(error)),
        }
    }
}

/// Apply one session tool-call limit increase and emit the shared grant
/// telemetry. Used by both the full-auto grant path and the manual prompt
/// path so the two converge on one event shape; only the message prefix
/// records whether a human approved the increase.
fn record_session_limit_grant(
    safety_validator: &ToolCallSafetyValidator,
    harness_state: Option<&mut HarnessTurnState>,
    harness_emitter: Option<&HarnessEventEmitter>,
    agent_name: Option<&str>,
    tool_name: &str,
    increment: usize,
    limit_increase_attempts: u32,
    auto_granted: bool,
) {
    safety_validator.increase_session_limit(increment);
    let new_limit = safety_validator.max_per_session();
    if let Some(state) = harness_state {
        state.record_session_limit_grant();
    }
    let message = if auto_granted {
        format!(
            "Full-auto auto-granted +{} session tool calls to current agent {} (limit {}); retrying the pending {} call in this turn. Reuse existing tool outputs for subsequent calls.",
            increment,
            agent_name.unwrap_or("unknown"),
            new_limit,
            tool_name,
        )
    } else {
        format!(
            "Current agent {} granted +{} session tool calls (limit {}); retrying the pending {} call in this turn. Reuse existing tool outputs for subsequent calls.",
            agent_name.unwrap_or("unknown"),
            increment,
            new_limit,
            tool_name,
        )
    };
    if auto_granted {
        tracing::info!(
            tool = %tool_name,
            increment,
            new_limit,
            "Full-auto granted a session tool-call limit increase without prompting",
        );
    }
    if let Some(emitter) = harness_emitter
        && let Err(error) = emitter.emit(harness_event(
            vtcode_core::exec::events::HarnessEventKind::SessionToolLimitIncreased,
            Some(message),
            None,
            Some(limit_increase_attempts),
            None,
        ))
    {
        tracing::debug!(error = %error, "Failed to emit session tool-limit grant event");
    }
}
