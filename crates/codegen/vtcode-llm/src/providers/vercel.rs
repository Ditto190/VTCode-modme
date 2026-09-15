use vtcode_config::constants::{env_vars, models, urls};

use super::openai_compat::{OpenAiCompatCore, OpenAiCompatSpec, impl_openai_compat_provider};

pub struct VercelSpec;

impl OpenAiCompatSpec for VercelSpec {
    const NAME: &'static str = "Vercel AI Gateway";
    const KEY: &'static str = "vercel";
    const API_KEY_ENV: &'static str = "AI_GATEWAY_API_KEY";
    const DEFAULT_MODEL: &'static str = models::vercel::DEFAULT_MODEL;
    const DEFAULT_BASE_URL: &'static str = urls::VERCEL_AI_GATEWAY_API_BASE;
    const BASE_URL_ENV: Option<&'static str> = Some(env_vars::VERCEL_AI_GATEWAY_BASE_URL);
    const LISTED_MODELS: &'static [&'static str] = models::vercel::SUPPORTED_MODELS;
    // The gateway routes hundreds of models that change independently of this
    // catalog, so only request shape is validated.
    const VALIDATION_ALLOWLIST: Option<&'static [&'static str]> = None;

    const SUPPRESS_SAMPLING_WHEN_REASONING: bool = false;
    const STREAM_OPTIONS_INCLUDE_USAGE: bool = true;
    // The gateway forwards OpenAI-style reasoning payloads in both fields
    // depending on the upstream vendor.
    const STREAM_REASONING_FIELDS: &'static [&'static str] = &["reasoning", "reasoning_content"];

    fn resolve_api_key(api_key: Option<String>) -> String {
        api_key
            .or_else(|| std::env::var(Self::API_KEY_ENV).ok().filter(|key| !key.trim().is_empty()))
            .unwrap_or_default()
    }
}

impl VercelProvider {
    /// AI Gateway serves OpenAI's standalone compaction endpoint
    /// (`POST /v1/responses/compact`, forwarded to OpenAI unchanged apart from
    /// the model ID), but only for OpenAI-routed models on the gateway's own
    /// endpoint. Other routes and custom base URLs stay on the universal local
    /// summarization fallback.
    fn vercel_compact_model(&self, model: &str) -> bool {
        let resolved = if model.trim().is_empty() {
            self.core.model.as_str()
        } else {
            model
        };
        resolved.starts_with("openai/") && self.core.base_url.contains("vercel.sh")
    }

    fn compact_client(&self, model: &str) -> crate::providers::openresponses::OpenResponsesProvider {
        crate::providers::openresponses::OpenResponsesProvider::compact_endpoint_client(
            &self.core.model,
            &self.core.base_url,
            &self.core.api_key,
            model,
        )
    }
}

impl_openai_compat_provider!(VercelProvider, VercelSpec, {
    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_structured_output(&self, _model: &str) -> bool {
        true
    }

    fn supports_reasoning(&self, model: &str) -> bool {
        use vtcode_config::constants::models;
        !models::vercel::NON_REASONING_MODELS.contains(&model)
    }

    fn effective_context_size(&self, model: &str) -> usize {
        crate::provider::catalog_context_window("vercel", model, 1_000_000)
    }

    fn supports_responses_compaction(&self, model: &str) -> bool {
        self.vercel_compact_model(model)
    }

    fn supports_manual_openai_compaction(&self, model: &str) -> bool {
        self.vercel_compact_model(model)
    }

    async fn compact_history(
        &self,
        model: &str,
        history: &[crate::provider::Message],
    ) -> Result<Vec<crate::provider::Message>, crate::provider::LLMError> {
        if !self.vercel_compact_model(model) {
            return Err(crate::provider::LLMError::Provider {
                message:
                    "Vercel AI Gateway compaction is only supported for OpenAI-routed models on the gateway endpoint"
                        .to_string(),
                metadata: None,
            });
        }
        self.compact_client(model).compact_history_request(model, history).await
    }

    async fn compact_history_with_options(
        &self,
        model: &str,
        history: &[crate::provider::Message],
        _options: &crate::provider::ResponsesCompactionOptions,
    ) -> Result<Vec<crate::provider::Message>, crate::provider::LLMError> {
        self.compact_history(model, history).await
    }
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{LLMRequest, Message, ToolChoice};
    use std::sync::Arc;
    use vtcode_config::types::ReasoningEffortLevel;

    fn provider() -> VercelProvider {
        VercelProvider::from_config(
            Some("test-key".to_string()),
            Some(models::vercel::ANTHROPIC_CLAUDE_SONNET_5.to_string()),
            Some("https://example.test/v1".to_string()),
            None,
            None,
            None,
            None,
        )
    }

    fn base_request() -> LLMRequest {
        LLMRequest {
            messages: vec![Message::user("hello".to_string())].into(),
            system_prompt: Some(Arc::from("system guidance")),
            model: models::vercel::ANTHROPIC_CLAUDE_SONNET_5.to_string(),
            max_tokens: Some(512),
            temperature: Some(0.5),
            stream: true,
            tool_choice: Some(ToolChoice::Auto),
            ..Default::default()
        }
    }

    #[test]
    fn golden_payload_basic_shape() {
        let payload = provider().core.convert_request(&base_request()).unwrap();

        assert_eq!(payload["model"], models::vercel::ANTHROPIC_CLAUDE_SONNET_5);
        let messages = payload["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "system guidance");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "hello");
        assert_eq!(payload["max_tokens"], 512);
        assert_eq!(payload["temperature"], 0.5);
        assert_eq!(payload["stream"], true);
        assert_eq!(payload["stream_options"]["include_usage"], true);
        assert_eq!(payload["tool_choice"], "auto");
    }

    #[test]
    fn golden_payload_reasoning_keeps_sampling() {
        let mut request = base_request();
        request.reasoning_effort = Some(ReasoningEffortLevel::High);
        let payload = provider().core.convert_request(&request).unwrap();
        assert_eq!(payload["temperature"], 0.5);
        assert!(payload.get("reasoning").is_none());
    }

    #[test]
    fn golden_payload_omits_empty_system_prompt() {
        let mut request = base_request();
        request.system_prompt = Some(Arc::from("   "));
        request.stream = false;
        let payload = provider().core.convert_request(&request).unwrap();
        let messages = payload["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert!(payload.get("stream").is_none());
        assert!(payload.get("stream_options").is_none());
    }

    #[test]
    fn gateway_ids_are_forwarded_verbatim() {
        let model = "some-vendor/unlisted-model";
        let provider =
            VercelProvider::from_config(Some("k".to_string()), Some(model.to_string()), None, None, None, None, None);
        assert_eq!(provider.core.model, model);
        assert_eq!(provider.core.base_url, urls::VERCEL_AI_GATEWAY_API_BASE);
    }

    #[test]
    fn compaction_support_is_openai_gateway_routes_only() {
        use crate::provider::LLMProvider;

        // Default gateway endpoint: OpenAI-routed models compact natively.
        let gateway = VercelProvider::from_config(
            Some("k".to_string()),
            Some("openai/gpt-6-astra".to_string()),
            None,
            None,
            None,
            None,
            None,
        );
        assert!(gateway.supports_responses_compaction("openai/gpt-6-astra"));
        assert!(gateway.supports_manual_openai_compaction("openai/gpt-6-astra"));
        assert!(!gateway.supports_responses_compaction("anthropic/claude-sonnet-5"));
        assert!(!gateway.supports_manual_openai_compaction("anthropic/claude-sonnet-5"));
        assert!(!gateway.supports_native_inline_compaction("openai/gpt-6-astra"));

        // Custom base URLs stay on local compaction even for OpenAI-routed
        // models: the compact endpoint only exists on the gateway itself.
        let custom = provider();
        assert!(!custom.supports_manual_openai_compaction("openai/gpt-6-astra"));
    }

    #[tokio::test]
    async fn compact_history_posts_to_gateway_compact_endpoint() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/responses/compact"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "resp_compact_1",
                "object": "response.compaction",
                "created_at": 1756800000,
                "output": [
                    {
                        "id": "msg_000",
                        "type": "message",
                        "status": "completed",
                        "role": "user",
                        "content": [{ "type": "input_text", "text": "Refactor the auth module." }]
                    },
                    {
                        "id": "cmp_001",
                        "type": "compaction",
                        "encrypted_content": "gAAAAABpM0Yj"
                    }
                ]
            })))
            .mount(&server)
            .await;

        // The host gate only passes on the gateway endpoint, so the transport
        // is exercised through the compact client directly against the mock.
        let provider = VercelProvider::from_config(
            Some("test-key".to_string()),
            Some("openai/gpt-6-astra".to_string()),
            Some(format!("{}/v1", server.uri())),
            None,
            None,
            None,
            None,
        );
        let history = vec![Message::user("Refactor the auth module.".to_string())];
        let compacted = provider
            .compact_client("openai/gpt-6-astra")
            .compact_history_request("openai/gpt-6-astra", &history)
            .await
            .expect("gateway compaction should succeed");
        assert!(!compacted.is_empty());
        assert!(
            compacted
                .iter()
                .any(|message| message.content.as_text().contains("Refactor the auth module.")),
            "retained gateway input must survive compaction"
        );
    }

    #[tokio::test]
    async fn compact_history_rejects_non_openai_routes() {
        use crate::provider::LLMProvider;

        let provider = provider();
        let history = vec![Message::user("hello".to_string())];
        provider
            .compact_history("anthropic/claude-sonnet-5", &history)
            .await
            .expect_err("non-OpenAI gateway routes must stay on local compaction");
    }
}
