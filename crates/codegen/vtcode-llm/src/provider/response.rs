use std::pin::Pin;

pub use vtcode_commons::llm::{FinishReason, LLMError, LLMResponse, Usage};

/// Provider-side classification for streamed reasoning text.
///
/// Only an explicit provider summary is safe to expose in the user interface.
/// Raw and continuation-only reasoning remains available on the response for
/// providers that require it on the next request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningSource {
    /// The provider explicitly marked this text as a public reasoning summary.
    ProviderSummary,
    /// The provider returned reasoning text without a public-summary marker.
    Raw,
    /// The provider returned reasoning metadata needed to continue a request.
    Continuation,
    /// The source was not classified by the provider adapter.
    Unknown,
}

impl ReasoningSource {
    #[must_use]
    pub const fn is_public_summary(self) -> bool {
        matches!(self, Self::ProviderSummary)
    }
}

#[derive(Debug, Clone)]
pub enum LLMStreamEvent {
    Token { delta: String },
    Reasoning { delta: String },
    ReasoningSignature { signature: String },
    ReasoningStage { stage: String },
    Completed { response: Box<LLMResponse> },
}

#[derive(Debug, Clone)]
pub enum NormalizedStreamEvent {
    TextDelta {
        delta: String,
    },
    ReasoningDelta {
        delta: String,
        source: ReasoningSource,
    },
    /// A provider-native reasoning stage transition.
    ReasoningStage {
        stage: String,
    },
    ToolCallStart {
        call_id: String,
        name: Option<String>,
    },
    ToolCallDelta {
        call_id: String,
        delta: String,
    },
    Usage {
        usage: Usage,
    },
    Done {
        response: Box<LLMResponse>,
    },
}

pub type LLMStream = Pin<Box<dyn futures::Stream<Item = Result<LLMStreamEvent, LLMError>> + Send>>;
pub type BorrowedLLMStream<'a> = Pin<Box<dyn futures::Stream<Item = Result<LLMStreamEvent, LLMError>> + Send + 'a>>;
pub type LLMNormalizedStream = Pin<Box<dyn futures::Stream<Item = Result<NormalizedStreamEvent, LLMError>> + Send>>;

impl LLMStreamEvent {
    pub(crate) fn into_normalized(self) -> Vec<NormalizedStreamEvent> {
        match self {
            Self::Token { delta } => vec![NormalizedStreamEvent::TextDelta { delta }],
            Self::Reasoning { delta } => {
                vec![NormalizedStreamEvent::ReasoningDelta { delta, source: ReasoningSource::Unknown }]
            }
            Self::ReasoningSignature { .. } => Vec::new(),
            Self::ReasoningStage { stage } => vec![NormalizedStreamEvent::ReasoningStage { stage }],
            Self::Completed { response } => {
                let mut events = Vec::new();
                if let Some(usage) = response.usage.clone() {
                    events.push(NormalizedStreamEvent::Usage { usage });
                }
                events.push(NormalizedStreamEvent::Done { response });
                events
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FinishReason, LLMResponse, LLMStreamEvent, NormalizedStreamEvent, ReasoningSource, Usage};

    #[test]
    fn completed_event_emits_usage_before_done() {
        let events = LLMStreamEvent::Completed {
            response: Box::new(LLMResponse {
                content: Some("done".to_string()),
                model: "gpt-5.6-sol".to_string(),
                tool_calls: None,
                usage: Some(Usage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                    cached_prompt_tokens: None,
                    cache_creation_tokens: None,
                    cache_read_tokens: None,
                    iterations: None,
                }),
                finish_reason: FinishReason::Stop,
                reasoning: None,
                reasoning_details: None,
                organization_id: None,
                request_id: None,
                tool_references: Vec::new(),
                compaction: None,
            }),
        }
        .into_normalized();

        assert!(matches!(events.first(), Some(NormalizedStreamEvent::Usage { .. })));
        assert!(matches!(events.last(), Some(NormalizedStreamEvent::Done { .. })));
    }

    #[test]
    fn token_event_maps_to_text_delta() {
        let events = LLMStreamEvent::Token { delta: "hello".to_string() }.into_normalized();

        assert!(matches!(
            events.as_slice(),
            [NormalizedStreamEvent::TextDelta { delta }] if delta == "hello"
        ));
    }

    #[test]
    fn reasoning_stage_is_preserved_by_normalization() {
        let events = LLMStreamEvent::ReasoningStage { stage: "analysis".to_string() }.into_normalized();

        assert!(matches!(
            events.as_slice(),
            [NormalizedStreamEvent::ReasoningStage { stage }] if stage == "analysis"
        ));
    }

    #[test]
    fn legacy_reasoning_events_are_unknown_and_not_public_summaries() {
        let events = LLMStreamEvent::Reasoning { delta: "private".to_string() }.into_normalized();

        assert!(matches!(
            events.as_slice(),
            [NormalizedStreamEvent::ReasoningDelta { delta, source }]
                if delta == "private" && *source == ReasoningSource::Unknown && !source.is_public_summary()
        ));
    }
}
