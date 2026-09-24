use crate::provider::{AnthropicThinkingDisplayOverride, AnthropicThinkingModeOverride, LLMRequest};
use crate::providers::anthropic_types::{ThinkingConfig, ThinkingDisplay};
use crate::rig_adapter::RigProviderCapabilities;
use serde_json::{Value, json};
use std::env;
use tracing::warn;
use vtcode_config::constants::env_vars;
use vtcode_config::core::AnthropicConfig;
use vtcode_config::models::Provider;
use vtcode_config::types::ReasoningEffortLevel;

use vtcode_config::constants::models::anthropic;

use super::super::capabilities::{
    ClaudeThinkingProfile, claude_thinking_profile, default_max_tokens_for_model, effort_is_at_most_high,
    effort_str_is_at_most_high, matches_model, resolve_model_name, supports_reasoning_effort,
};

fn resolve_configured_thinking_display(anthropic_config: &AnthropicConfig) -> Option<ThinkingDisplay> {
    anthropic_config.thinking_display.and_then(|d| match d {
        vtcode_config::ThinkingDisplayMode::Summarized => Some(ThinkingDisplay::Summarized),
        vtcode_config::ThinkingDisplayMode::Omitted => Some(ThinkingDisplay::Omitted),
        vtcode_config::ThinkingDisplayMode::Unknown => None,
    })
}

fn resolve_thinking_display(request: &LLMRequest, anthropic_config: &AnthropicConfig) -> Option<ThinkingDisplay> {
    if let Some(overrides) = request.anthropic_request_overrides.as_ref() {
        return match overrides.thinking_display {
            AnthropicThinkingDisplayOverride::Inherit => None,
            AnthropicThinkingDisplayOverride::Summarized => Some(ThinkingDisplay::Summarized),
            AnthropicThinkingDisplayOverride::Omitted => Some(ThinkingDisplay::Omitted),
        };
    }

    resolve_configured_thinking_display(anthropic_config)
}

/// Builds a manual `budget_tokens` config clamped below `max_tokens`, which
/// must be the value the request will actually send.
fn manual_thinking_config(budget: u32, max_tokens: u32, display: Option<ThinkingDisplay>) -> Option<ThinkingConfig> {
    if budget < 1024 {
        return None;
    }

    let effective_budget = budget.min(max_tokens.saturating_sub(100)).max(1024);
    Some(ThinkingConfig::Enabled { budget_tokens: effective_budget, display })
}

/// Whether a profiled model accepts `thinking: {type: "disabled"}`.
/// Adaptive-only models (Opus 5.5, Fable 5.x) reject it outright, and Opus 5
/// accepts it only at effort `high` or below. The Opus 5.5 id contains the
/// Opus 5 id, so the adaptive-only check must run first.
fn disabled_thinking_allowed(
    profile: &ClaudeThinkingProfile,
    model: &str,
    effort_is_at_most_high: impl FnOnce() -> bool,
) -> bool {
    if profile.adaptive_only {
        return false;
    }
    if matches_model(model, anthropic::CLAUDE_OPUS_5) {
        return effort_is_at_most_high();
    }
    true
}

/// Rewrites `thinking` into a config that is valid as a direct request to
/// `model`, or returns `None` when it is already valid (or the model has no
/// capability profile to check against).
///
/// Server-side fallback entries are merged into the primary request, so the
/// merged request must satisfy the fallback model's own rules: models without
/// manual-budget support get adaptive thinking instead of `budget_tokens`, and
/// a `disabled` config the model rejects becomes adaptive. `effort` is the
/// request's `output_config.effort`, which fallback entries inherit; `None`
/// means the model's default effort applies.
pub(crate) fn rewrite_thinking_for_model(
    thinking: &ThinkingConfig,
    model: &str,
    default_model: &str,
    effort: Option<&str>,
) -> Option<ThinkingConfig> {
    let resolved_model = resolve_model_name(model, default_model);
    let profile = claude_thinking_profile(resolved_model, default_model)?;
    match thinking {
        ThinkingConfig::Enabled { display, .. } if !profile.supports_manual_budget => {
            Some(ThinkingConfig::Adaptive { display: *display })
        }
        ThinkingConfig::Disabled
            if !disabled_thinking_allowed(&profile, resolved_model, || {
                effort_str_is_at_most_high(effort.unwrap_or(profile.default_effort))
            }) =>
        {
            Some(ThinkingConfig::Adaptive { display: None })
        }
        _ => None,
    }
}

pub(crate) fn build_thinking_config(
    request: &LLMRequest,
    anthropic_config: &AnthropicConfig,
    default_model: &str,
) -> Result<(Option<ThinkingConfig>, Option<Value>), crate::provider::LLMError> {
    let resolved_model = resolve_model_name(&request.model, default_model);
    let profile = claude_thinking_profile(resolved_model, default_model);
    let display = resolve_thinking_display(request, anthropic_config);
    let default_thinking = profile.is_some_and(|p| p.default_thinking_enabled);
    // The request builder sends this same default whenever thinking is on, so
    // manual budgets are clamped against the real `max_tokens`.
    let thinking_max_tokens = request
        .max_tokens
        .unwrap_or_else(|| default_max_tokens_for_model(resolved_model, default_model, true));

    if let Some(overrides) = request.anthropic_request_overrides.as_ref() {
        match overrides.thinking_mode {
            AnthropicThinkingModeOverride::Disabled => {
                // Models that think by default need an explicit `disabled`
                // when they accept one; otherwise the field is omitted and the
                // model runs its default thinking mode.
                if let Some(profile) = profile.filter(|p| p.default_thinking_enabled)
                    && disabled_thinking_allowed(&profile, resolved_model, || {
                        effort_is_at_most_high(request, anthropic_config)
                    })
                {
                    return Ok((Some(ThinkingConfig::Disabled), None));
                }
                return Ok((None, None));
            }
            AnthropicThinkingModeOverride::Adaptive => {
                return Ok((Some(ThinkingConfig::Adaptive { display }), None));
            }
            AnthropicThinkingModeOverride::ManualBudget(budget) => {
                // Profiled models without manual-budget support (every
                // current Claude 5.x model) reject `budget_tokens` with a
                // 400, so an explicit budget is served as adaptive thinking.
                if profile.is_some_and(|p| !p.supports_manual_budget) {
                    return Ok((Some(ThinkingConfig::Adaptive { display }), None));
                }
                return Ok((manual_thinking_config(budget, thinking_max_tokens, display), None));
            }
            AnthropicThinkingModeOverride::Inherit => {}
        }
    }

    let thinking_enabled = if default_thinking {
        if !anthropic_config.extended_thinking_enabled {
            tracing::warn!(
                model = %request.model,
                "extended_thinking_enabled=false overridden by model default thinking profile; thinking will be enabled"
            );
        }
        true
    } else {
        anthropic_config.extended_thinking_enabled && supports_reasoning_effort(resolved_model, default_model)
    };

    if thinking_enabled {
        if profile.is_some_and(|p| matches!(p.mode, super::super::capabilities::ClaudeThinkingMode::Adaptive)) {
            if profile.is_some_and(|p| p.supports_manual_budget)
                && let Some(explicit_budget) = request.thinking_budget
            {
                return Ok((manual_thinking_config(explicit_budget, thinking_max_tokens, display), None));
            }
            return Ok((Some(ThinkingConfig::Adaptive { display }), None));
        }

        let max_thinking_tokens: Option<u32> =
            env::var(env_vars::MAX_THINKING_TOKENS).ok().and_then(|v| v.parse().ok());

        let budget = if let Some(explicit_budget) = request.thinking_budget {
            explicit_budget
        } else if let Some(env_budget) = max_thinking_tokens {
            env_budget
        } else if let Some(effort) = request.reasoning_effort {
            match effort {
                ReasoningEffortLevel::None | ReasoningEffortLevel::Unknown => 0,
                ReasoningEffortLevel::Minimal => 1024,
                ReasoningEffortLevel::Low => 4096,
                ReasoningEffortLevel::Medium => 8192,
                ReasoningEffortLevel::High => 16384,
                ReasoningEffortLevel::XHigh => 32768,
                ReasoningEffortLevel::Max => 32768,
            }
        } else {
            anthropic_config.interleaved_thinking_budget_tokens
        };

        if let Some(thinking) = manual_thinking_config(budget, thinking_max_tokens, display) {
            return Ok((Some(thinking), None));
        }
    } else if let Some(effort) = request.reasoning_effort {
        if profile.is_some_and(|p| matches!(p.mode, super::super::capabilities::ClaudeThinkingMode::Adaptive)) {
            return Ok((None, None));
        }

        if let Some(payload) =
            RigProviderCapabilities::new(Provider::Anthropic, &request.model).reasoning_parameters(effort)?
        {
            return Ok((None, Some(payload)));
        } else {
            return Ok((None, Some(json!({ "effort": effort.as_str() }))));
        }
    }

    Ok((None, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtcode_config::constants::models::anthropic;

    #[test]
    fn ignores_explicit_budget_for_opus_4_8() {
        let request = LLMRequest {
            model: anthropic::CLAUDE_OPUS_5.to_string(),
            thinking_budget: Some(2048),
            ..Default::default()
        };
        let config = AnthropicConfig::default();
        let (thinking, _) =
            build_thinking_config(&request, &config, anthropic::DEFAULT_MODEL).expect("thinking config");

        assert!(matches!(thinking, Some(ThinkingConfig::Adaptive { .. })));
    }

    fn manual_budget_override_request(model: &str, budget: u32) -> LLMRequest {
        LLMRequest {
            model: model.to_string(),
            anthropic_request_overrides: Some(crate::provider::AnthropicRequestOverrides {
                thinking_mode: AnthropicThinkingModeOverride::ManualBudget(budget),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn manual_budget_override_becomes_adaptive_for_opus_5_5() {
        let request = manual_budget_override_request(anthropic::CLAUDE_OPUS_5_5, 4096);
        let config = AnthropicConfig::default();
        let (thinking, reasoning) =
            build_thinking_config(&request, &config, anthropic::DEFAULT_MODEL).expect("thinking config");

        assert!(matches!(thinking, Some(ThinkingConfig::Adaptive { display: None })), "got {thinking:?}");
        assert!(reasoning.is_none());
    }

    #[test]
    fn manual_budget_override_becomes_adaptive_for_every_profiled_model() {
        let config = AnthropicConfig::default();
        for model in [
            anthropic::CLAUDE_SONNET_5,
            anthropic::CLAUDE_OPUS_5,
            anthropic::CLAUDE_OPUS_5_5,
            anthropic::CLAUDE_FABLE_5,
            anthropic::CLAUDE_FABLE_5_1,
        ] {
            let request = manual_budget_override_request(model, 8192);
            let (thinking, _) =
                build_thinking_config(&request, &config, anthropic::DEFAULT_MODEL).expect("thinking config");
            assert!(
                matches!(thinking, Some(ThinkingConfig::Adaptive { .. })),
                "{model} must not receive budget_tokens, got {thinking:?}"
            );
        }
    }

    #[test]
    fn manual_budget_override_keeps_display_when_downgraded_to_adaptive() {
        let mut request = manual_budget_override_request(anthropic::CLAUDE_OPUS_5_5, 4096);
        if let Some(overrides) = request.anthropic_request_overrides.as_mut() {
            overrides.thinking_display = AnthropicThinkingDisplayOverride::Summarized;
        }
        let config = AnthropicConfig::default();
        let (thinking, _) =
            build_thinking_config(&request, &config, anthropic::DEFAULT_MODEL).expect("thinking config");

        assert!(
            matches!(thinking, Some(ThinkingConfig::Adaptive { display: Some(ThinkingDisplay::Summarized) })),
            "got {thinking:?}"
        );
    }

    #[test]
    fn manual_budget_override_is_kept_for_unprofiled_models() {
        let request = manual_budget_override_request("claude-unlisted-model", 4096);
        let config = AnthropicConfig::default();
        let (thinking, _) =
            build_thinking_config(&request, &config, anthropic::DEFAULT_MODEL).expect("thinking config");

        assert!(matches!(thinking, Some(ThinkingConfig::Enabled { budget_tokens: 4096, .. })), "got {thinking:?}");
    }

    #[test]
    fn adaptive_thinking_includes_summarized_display() {
        let request = LLMRequest {
            model: anthropic::CLAUDE_SONNET_5.to_string(),
            ..Default::default()
        };
        let config = AnthropicConfig {
            thinking_display: Some(vtcode_config::ThinkingDisplayMode::Summarized),
            ..AnthropicConfig::default()
        };
        let (thinking, _) =
            build_thinking_config(&request, &config, anthropic::DEFAULT_MODEL).expect("thinking config");

        match thinking {
            Some(ThinkingConfig::Adaptive { display: Some(ThinkingDisplay::Summarized) }) => {}
            other => panic!("expected Adaptive with Summarized display, got {other:?}"),
        }
    }

    #[test]
    fn adaptive_thinking_includes_omitted_display() {
        let request = LLMRequest {
            model: anthropic::CLAUDE_SONNET_5.to_string(),
            ..Default::default()
        };
        let config = AnthropicConfig {
            thinking_display: Some(vtcode_config::ThinkingDisplayMode::Omitted),
            ..AnthropicConfig::default()
        };
        let (thinking, _) =
            build_thinking_config(&request, &config, anthropic::DEFAULT_MODEL).expect("thinking config");

        match thinking {
            Some(ThinkingConfig::Adaptive { display: Some(ThinkingDisplay::Omitted) }) => {}
            other => panic!("expected Adaptive with Omitted display, got {other:?}"),
        }
    }

    #[test]
    fn adaptive_thinking_includes_display_for_sonnet_5_when_configured() {
        let request = LLMRequest {
            model: anthropic::CLAUDE_SONNET_5.to_string(),
            ..Default::default()
        };
        let config = AnthropicConfig {
            thinking_display: Some(vtcode_config::ThinkingDisplayMode::Summarized),
            ..AnthropicConfig::default()
        };
        let (thinking, _) =
            build_thinking_config(&request, &config, anthropic::DEFAULT_MODEL).expect("thinking config");

        match thinking {
            Some(ThinkingConfig::Adaptive { display: Some(ThinkingDisplay::Summarized) }) => {}
            other => panic!("expected Adaptive with Summarized display, got {other:?}"),
        }
    }

    #[test]
    fn thinking_display_defaults_to_none() {
        let request = LLMRequest {
            model: anthropic::CLAUDE_SONNET_5.to_string(),
            ..Default::default()
        };
        let config = AnthropicConfig::default();
        let (thinking, _) =
            build_thinking_config(&request, &config, anthropic::DEFAULT_MODEL).expect("thinking config");

        match thinking {
            Some(ThinkingConfig::Adaptive { display: None }) => {}
            other => panic!("expected Adaptive with no display, got {other:?}"),
        }
    }
}
