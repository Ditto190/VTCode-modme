use serde_json::{Map, Value};
use vtcode_config::constants::{env_vars, models, urls};

use super::openai_compat::{OpenAiCompatCore, OpenAiCompatSpec, SystemPromptPlacement, impl_openai_compat_provider};
use crate::provider::{LLMError, LLMRequest};

pub struct MoonshotSpec;

fn is_thinking_model(model: &str) -> bool {
    model.contains("kimi-k3") || model.contains("k2-thinking") || model.contains("kimi-k2-thinking")
}

/// Map portable effort levels onto Kimi K3's native `low` / `high` / `max` scale.
fn kimi_k3_effort_value(effort: vtcode_config::types::ReasoningEffortLevel) -> Option<&'static str> {
    match effort {
        vtcode_config::types::ReasoningEffortLevel::None | vtcode_config::types::ReasoningEffortLevel::Unknown => None,
        vtcode_config::types::ReasoningEffortLevel::Minimal | vtcode_config::types::ReasoningEffortLevel::Low => {
            Some("low")
        }
        vtcode_config::types::ReasoningEffortLevel::Medium | vtcode_config::types::ReasoningEffortLevel::High => {
            Some("high")
        }
        vtcode_config::types::ReasoningEffortLevel::XHigh | vtcode_config::types::ReasoningEffortLevel::Max => {
            Some("max")
        }
    }
}

impl OpenAiCompatSpec for MoonshotSpec {
    const NAME: &'static str = "Moonshot";
    const KEY: &'static str = "moonshot";
    const API_KEY_ENV: &'static str = "MOONSHOT_API_KEY";
    const DEFAULT_MODEL: &'static str = models::moonshot::DEFAULT_MODEL;
    const DEFAULT_BASE_URL: &'static str = urls::MOONSHOT_API_BASE;
    const BASE_URL_ENV: Option<&'static str> = Some(env_vars::MOONSHOT_BASE_URL);
    const LISTED_MODELS: &'static [&'static str] = models::moonshot::SUPPORTED_MODELS;
    // Moonshot publishes new official aliases and preview slugs faster than VT Code's
    // curated picker list is refreshed, so let the upstream API be the source of truth
    // for model identifiers and keep local validation focused on request shape.
    const VALIDATION_ALLOWLIST: Option<&'static [&'static str]> = None;

    const SYSTEM_PROMPT: SystemPromptPlacement = SystemPromptPlacement::Omitted;
    const INCLUDE_TOP_P: bool = false;
    const SUPPRESS_SAMPLING_WHEN_REASONING: bool = false;

    fn normalize_model(model: String) -> String {
        model.trim().to_string()
    }

    fn insert_reasoning(
        _core: &OpenAiCompatCore<Self>,
        request: &LLMRequest,
        payload: &mut Map<String, Value>,
    ) -> Result<(), LLMError> {
        // Kimi K3 accepts only `low`, `high`, and `max`. Kimi K2.7 Code omits
        // effort fields (always-on native thinking).
        if let Some(effort) = request.reasoning_effort
            && is_thinking_model(&request.model)
            && let Some(value) = kimi_k3_effort_value(effort)
        {
            payload.insert("reasoning_effort".to_string(), Value::String(value.to_string()));
        }
        Ok(())
    }
}

impl_openai_compat_provider!(MoonshotProvider, MoonshotSpec, {
    fn supports_reasoning(&self, model: &str) -> bool {
        is_thinking_model(model)
    }

    fn supports_reasoning_effort(&self, model: &str) -> bool {
        is_thinking_model(model)
    }
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Message, ToolChoice};
    use std::sync::Arc;
    use vtcode_config::types::ReasoningEffortLevel;

    fn provider() -> MoonshotProvider {
        MoonshotProvider::from_config(
            Some("test-key".to_string()),
            Some("kimi-k2.7".to_string()),
            Some("https://example.test/v1".to_string()),
            None,
            None,
            None,
            None,
        )
    }

    fn base_request(model: &str) -> LLMRequest {
        LLMRequest {
            messages: vec![Message::user("hello".to_string())].into(),
            system_prompt: Some(Arc::from("system guidance")),
            model: model.to_string(),
            max_tokens: Some(512),
            temperature: Some(0.5),
            stream: true,
            tool_choice: Some(ToolChoice::Auto),
            ..Default::default()
        }
    }

    #[test]
    fn golden_payload_basic_shape() {
        let payload = provider().core.convert_request(&base_request("kimi-k2.7")).unwrap();

        assert_eq!(payload["model"], "kimi-k2.7");
        // Moonshot does not inject the system prompt into messages.
        let messages = payload["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "hello");
        assert_eq!(payload["max_tokens"], 512);
        assert_eq!(payload["temperature"], 0.5);
        assert_eq!(payload["stream"], true);
        assert!(payload.get("stream_options").is_none());
        assert!(payload.get("top_p").is_none());
        assert!(payload.get("reasoning_effort").is_none());
        assert_eq!(payload["tool_choice"], "auto");
    }

    #[test]
    fn golden_payload_reasoning_effort_for_thinking_models() {
        let mut request = base_request("kimi-k2-thinking");
        request.reasoning_effort = Some(ReasoningEffortLevel::Low);
        let payload = provider().core.convert_request(&request).unwrap();
        assert_eq!(payload["reasoning_effort"], "low");
        // Sampling parameters are not suppressed for reasoning requests.
        assert_eq!(payload["temperature"], 0.5);

        let mut request = base_request("kimi-k2.7");
        request.reasoning_effort = Some(ReasoningEffortLevel::Low);
        let payload = provider().core.convert_request(&request).unwrap();
        assert!(payload.get("reasoning_effort").is_none());
    }

    #[test]
    fn kimi_k3_effort_maps_to_native_scale() {
        for (effort, expected) in [
            (ReasoningEffortLevel::Minimal, "low"),
            (ReasoningEffortLevel::Low, "low"),
            (ReasoningEffortLevel::Medium, "high"),
            (ReasoningEffortLevel::High, "high"),
            (ReasoningEffortLevel::XHigh, "max"),
            (ReasoningEffortLevel::Max, "max"),
        ] {
            let mut request = base_request("kimi-k3");
            request.reasoning_effort = Some(effort);
            let payload = provider().core.convert_request(&request).unwrap();
            assert_eq!(payload["reasoning_effort"], expected);
        }

        // Kimi K2.7 Code always uses native thinking; effort fields are omitted.
        let mut request = base_request("kimi-k2.7-code");
        request.reasoning_effort = Some(ReasoningEffortLevel::Max);
        let payload = provider().core.convert_request(&request).unwrap();
        assert!(payload.get("reasoning_effort").is_none());
    }
}
