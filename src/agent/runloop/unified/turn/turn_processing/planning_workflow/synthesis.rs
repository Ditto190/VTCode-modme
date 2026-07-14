//! LLM-backed synthesis of planning-workflow interview arguments.
//!
//! Interface contract: this module is the only place in `planning_workflow/`
//! that issues an LLM call to *generate* interview questions (as opposed to
//! `interview_forcing`, which only injects an already-built tool call into a
//! [`TurnProcessingResult`]). It never mutates `plan_session` and never
//! decides *whether* an interview is needed — that is
//! `super::gating`'s job. Keeping synthesis pure I/O plus a public
//! `synthesize_planning_workflow_interview_args` entrypoint means the
//! gating/need-state logic stays testable without a provider client.
use serde_json::Value;
use vtcode_core::llm::provider as uni;
use vtcode_core::retry::{RetryPolicy, RetryPolicyCoreExt};
use vtcode_core::tools::handlers::planning_workflow::{
    PlanningWorkflowState, validate_plan_content,
};

use super::CUSTOM_NOTE_POLICY;
use super::interview_context::{
    collect_interview_research_context, load_plan_draft_context, select_best_plan_validation,
};
use super::interview_payload::{
    build_adaptive_fallback_interview_args, parse_interview_payload_from_text,
    sanitize_generated_interview_payload, single_line,
};
use crate::agent::runloop::unified::plan_blocks::extract_any_plan;
use crate::agent::runloop::unified::state::SessionStats;

/// Synthesize the planning-workflow interview arguments, retrying transient
/// (network / 5xx / timeout / rate-limit) provider errors before giving up.
///
/// The interview-argument synthesis LLM call previously had no retry of its
/// own, so a single transient network blip could abort plan mode entirely. We
/// now retry with the canonical [`RetryPolicy`] backoff. If every attempt
/// fails we surface the error to the caller, which falls back to an adaptive
/// interview rather than dead-ending the planning session.
pub(super) async fn synthesize_interview_args_with_retry(
    provider_client: &mut Box<dyn uni::LLMProvider>,
    request: uni::LLMRequest,
) -> anyhow::Result<uni::LLMResponse> {
    let policy = RetryPolicy::default();
    let mut attempt: u32 = 0;
    loop {
        match provider_client.generate(request.clone()).await {
            Ok(response) => return Ok(response),
            Err(err) => {
                let anyhow_err = anyhow::Error::new(err);
                let decision = policy.decision_for_anyhow(&anyhow_err, attempt, None);
                if !decision.retryable {
                    return Err(anyhow_err);
                }
                if let Some(delay) = decision.delay {
                    tokio::time::sleep(delay).await;
                }
                attempt = attempt.saturating_add(1);
            }
        }
    }
}

pub(crate) async fn synthesize_planning_workflow_interview_args(
    provider_client: &mut Box<dyn uni::LLMProvider>,
    active_model: &str,
    working_history: &[uni::Message],
    response_text: Option<&str>,
    session_stats: &SessionStats,
    plan_state: Option<PlanningWorkflowState>,
) -> Option<Value> {
    let plan_context = load_plan_draft_context(plan_state).await;
    let context = collect_interview_research_context(
        working_history,
        response_text,
        session_stats,
        plan_context.as_ref(),
    );
    let latest_user_request = working_history
        .iter()
        .rev()
        .find(|message| message.role == uni::MessageRole::User)
        .map(|message| single_line(message.content.as_text().as_ref()))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "(none)".to_string());
    let system_prompt = format!(
        "You generate Planning workflow interview payloads for request_user_input.\n\
Return strict JSON only (no markdown/prose).\n\
The top-level value MUST be an object with a \"questions\" key whose value is a flat JSON array of question objects.\n\
Example: {{\"questions\": [{{\"id\": \"scope\", \"header\": \"Scope\", \"question\": \"...\", \"options\": [{{\"label\": \"A (Recommended)\", \"description\": \"...\"}}, {{\"label\": \"B\", \"description\": \"...\"}}]}}]}}\n\
Do NOT wrap the array in an extra container like {{\"item\": [...]}} or {{\"questions\": {{\"item\": [...]}}}}.\n\
Constraints:\n\
- 1 to 3 questions\n\
- each question: id snake_case, header <= 12 chars, question is one line\n\
- each question options: 2 or 3 mutually-exclusive options\n\
- recommended option first and include '(Recommended)' in its label\n\
- {CUSTOM_NOTE_POLICY}\n\
Use repository research context to ask questions that close planning decisions for scope, decomposition, and verification."
    );
    let user_prompt = format!(
        "Build context-aware interview questions for this planning state.\n\
Current user request:\n{}\n\
Research context JSON:\n{}\n\
Assistant response snapshot:\n{}\n\
Return JSON only.",
        latest_user_request,
        serde_json::to_string_pretty(&context).ok()?,
        response_text.unwrap_or("(none)")
    );

    let request = uni::LLMRequest {
        messages: vec![uni::Message::user(user_prompt)],
        system_prompt: Some(std::sync::Arc::new(system_prompt)),
        tools: None,
        model: active_model.to_string(),
        temperature: Some(0.2),
        stream: false,
        max_tokens: Some(1024),
        ..Default::default()
    };

    let generated = synthesize_interview_args_with_retry(provider_client, request)
        .await
        .inspect_err(|err| {
            tracing::warn!(
                error = %err,
                "Interview-arg synthesis failed; falling back to adaptive interview"
            );
        })
        .ok()
        .and_then(|response| response.content)
        .and_then(|content| parse_interview_payload_from_text(&content))
        .and_then(|payload| sanitize_generated_interview_payload(payload, &context));

    let response_plan_validation = response_text
        .and_then(|text| extract_any_plan(text).plan_text)
        .as_deref()
        .map(validate_plan_content);
    let plan_validation = select_best_plan_validation(
        plan_context
            .as_ref()
            .and_then(|ctx| ctx.plan_validation.as_ref()),
        response_plan_validation.as_ref(),
    );
    let tracker_summary = plan_context
        .as_ref()
        .and_then(|ctx| ctx.tracker_summary.clone());

    generated.or_else(|| {
        build_adaptive_fallback_interview_args(
            &context,
            response_text,
            plan_validation,
            tracker_summary,
        )
    })
}
