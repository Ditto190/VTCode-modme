use vtcode_core::llm::provider as uni;
use vtcode_core::tools::tool_intent;

use crate::agent::runloop::unified::turn::context::TurnProcessingContext;

pub(crate) fn record_tool_call_budget_usage(
    ctx: &mut TurnProcessingContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) {
    ctx.harness_state.record_admitted_tool_call();
    // Control-plane exec calls (blocking `wait`, bounded `inspect`) coordinate
    // long-running commands without producing new work, so they must not
    // consume the per-turn tool-call budget their follow-up work needs.
    // Otherwise a long build's wait/poll/inspect cadence exhausts the budget
    // before verification can run. The gateway keeps its own bounded cap and
    // the session fuse, so the exemption cannot become the dominant path.
    if tool_intent::is_turn_budget_exempt_call(tool_name, args) {
        return;
    }
    if let Some(warning) = ctx.harness_state.record_tool_call_with_default_warning() {
        ctx.working_history.push(uni::Message::system(warning.system_message()));
    }
}
