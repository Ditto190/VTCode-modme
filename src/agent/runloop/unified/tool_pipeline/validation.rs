use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::Notify;
use vtcode_core::core::interfaces::ui::UiSession;
use vtcode_core::tools::ToolInvocationId;
use vtcode_ui::tui::app::InlineHandle;

use crate::agent::runloop::unified::inline_events::harness::{HarnessEventEmitter, harness_event};
use crate::agent::runloop::unified::run_loop_context::HarnessTurnState;
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
                match prompt_session_limit_increase(handle, session, ctrl_c_state, ctrl_c_notify, max, agent_name).await
                {
                    Ok(Some(increment)) => {
                        safety_validator.increase_session_limit(increment);
                        let new_limit = safety_validator.max_per_session();
                        if let Some(state) = harness_state.as_deref_mut() {
                            state.record_session_limit_grant();
                        }
                        if let Some(emitter) = harness_emitter
                            && let Err(error) = emitter.emit(harness_event(
                                vtcode_core::exec::events::HarnessEventKind::SessionToolLimitIncreased,
                                Some(format!(
                                    "Current agent {} granted +{} session tool calls (limit {}); retrying the pending {} call in this turn. Reuse existing tool outputs for subsequent calls.",
                                    agent_name.unwrap_or("unknown"),
                                    increment,
                                    new_limit,
                                    tool_name,
                                )),
                                None,
                                Some(limit_increase_attempts),
                                None,
                            ))
                        {
                            tracing::debug!(error = %error, "Failed to emit session tool-limit grant event");
                        }
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
