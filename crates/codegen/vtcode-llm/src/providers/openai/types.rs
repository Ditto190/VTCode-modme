//! OpenAI provider types and constants.
//!
//! This module contains shared types used across the OpenAI provider implementation.

use serde_json::Value;

/// Responses API availability state for a given model.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResponsesApiState {
    /// Responses API is required for this model (e.g., GPT-5 Codex).
    Required,
    /// Responses API is allowed but not required.
    Allowed,
    /// Responses API is disabled (use Chat Completions).
    Disabled,
}

/// Payload structure for OpenAI Responses API requests.
pub(crate) struct OpenAIResponsesPayload {
    /// The input messages/items for the request.
    pub(crate) input: Vec<Value>,
    /// Optional system instructions.
    pub(crate) instructions: Option<String>,
    /// Per-segment provenance for `instructions`, in wire order.
    ///
    /// `None` when `instructions` is `None` or when the composition is
    /// all-static (single stable segment). Consumers that need to relocate
    /// volatile content use this instead of guessing at a joined-string
    /// layout, which breaks the moment segment order varies.
    pub(crate) instruction_segments: Option<Vec<(InstructionSegmentKind, String)>>,
}

/// Provenance kind for one segment of the composed `instructions` string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstructionSegmentKind {
    /// The request-level system prompt (session-stable).
    SystemPrompt,
    /// A `System`-role message from conversation history (per-turn).
    HistorySystem,
    /// Assistant/tool history folded into instructions (non-structured path).
    FoldedHistory,
}

/// Maximum completion tokens field name for Chat Completions API.
pub(crate) const MAX_COMPLETION_TOKENS_FIELD: &str = "max_completion_tokens";
