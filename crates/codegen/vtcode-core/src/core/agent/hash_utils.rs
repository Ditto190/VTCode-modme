//! Hash utilities for tool catalog and system prompt caching.
//!
//! Provides hashing functions for tool definitions, system prompts, and
//! low-signal attempt deduplication keys.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::Serialize;
use serde_json::Value;

use crate::llm::provider::ToolDefinition;

/// Resolved wire capabilities that define a cache-stable request segment.
/// Runtime counters and environment observations deliberately do not belong here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PromptCapabilityIdentity {
    provider: compact_str::CompactString,
    model: compact_str::CompactString,
    context_window: usize,
    reasoning_tag: compact_str::CompactString,
    reasoning_effort: bool,
    parallel_tools: bool,
    tools: bool,
    caching: bool,
    catalog_epoch: u64,
}

impl PromptCapabilityIdentity {
    /// Prefer discovery/catalog capacity when the caller already resolved a model.
    #[must_use]
    pub fn with_resolved_model(mut self, resolved: &crate::llm::model_resolver::ResolvedModel) -> Self {
        self.context_window = resolved.context_window().unwrap_or(self.context_window);
        self
    }
    #[must_use]
    pub fn resolve(
        provider: &dyn crate::llm::provider::LLMProvider,
        model: &str,
        reasoning: Option<crate::config::types::ReasoningEffortLevel>,
        catalog_epoch: u64,
    ) -> Self {
        Self {
            provider: provider.name().into(),
            model: model.into(),
            context_window: provider.effective_context_size(model),
            reasoning_tag: reasoning.map_or_else(|| "unset".into(), |effort| effort.to_string().into()),
            reasoning_effort: provider.supports_reasoning_effort(model),
            parallel_tools: provider.supports_parallel_tool_config(model),
            tools: provider.supports_tools(model),
            caching: provider.supports_context_caching(model),
            catalog_epoch,
        }
    }

    #[must_use]
    pub fn prefix_hash(&self, prompt_hash: u64) -> u64 {
        hash_value(&(prompt_hash, self))
    }
}

#[cfg(test)]
mod capability_tests {
    use super::*;

    #[test]
    fn capability_identity_resolves_three_provider_families_without_transport() {
        use crate::config::constants::models;
        use crate::config::types::ReasoningEffortLevel;
        use crate::llm::provider::LLMProvider;
        use crate::llm::providers::{AnthropicProvider, GeminiProvider, OpenAIProvider};

        let providers: [(Box<dyn LLMProvider>, &str); 3] = [
            (Box::new(OpenAIProvider::new("offline-fixture".into())), models::openai::DEFAULT_MODEL),
            (Box::new(AnthropicProvider::new("offline-fixture".into())), models::anthropic::DEFAULT_MODEL),
            (Box::new(GeminiProvider::new("offline-fixture".into())), models::google::DEFAULT_MODEL),
        ];
        for (provider, model) in providers {
            let identity =
                PromptCapabilityIdentity::resolve(provider.as_ref(), model, Some(ReasoningEffortLevel::High), 1);
            assert_eq!(identity.context_window, provider.effective_context_size(model));
            assert_eq!(identity.parallel_tools, provider.supports_parallel_tool_config(model));
            assert_eq!(identity.reasoning_effort, provider.supports_reasoning_effort(model));
            let repeat =
                PromptCapabilityIdentity::resolve(provider.as_ref(), model, Some(ReasoningEffortLevel::High), 1);
            assert_eq!(identity.prefix_hash(7), repeat.prefix_hash(7));
            let refreshed =
                PromptCapabilityIdentity::resolve(provider.as_ref(), model, Some(ReasoningEffortLevel::High), 2);
            assert_ne!(identity.prefix_hash(7), refreshed.prefix_hash(7));
        }
    }

    #[test]
    fn capability_prefix_tracks_changes_for_three_provider_families() {
        for (provider, model, context_window) in [
            ("openai", "openai-fixture", 1_000_000),
            ("anthropic", "claude-fixture", 200_000),
            ("gemini", "gemini-fixture", 32_000),
        ] {
            let identity = PromptCapabilityIdentity {
                provider: provider.into(),
                model: model.into(),
                context_window,
                reasoning_tag: "high".into(),
                reasoning_effort: true,
                parallel_tools: true,
                tools: true,
                caching: true,
                catalog_epoch: 1,
            };
            let baseline = identity.prefix_hash(7);
            assert_eq!(baseline, identity.clone().prefix_hash(7));
            let mut changed = identity.clone();
            changed.catalog_epoch += 1;
            assert_ne!(baseline, changed.prefix_hash(7));
            changed = identity.clone();
            changed.reasoning_tag = "low".into();
            assert_ne!(baseline, changed.prefix_hash(7));
            changed = identity.clone();
            changed.context_window /= 2;
            assert_ne!(baseline, changed.prefix_hash(7));
            changed = identity;
            changed.parallel_tools = false;
            assert_ne!(baseline, changed.prefix_hash(7));
        }
    }
}

/// Hash a value using the default hasher.
pub fn hash_value<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// Hash a serializable value as JSON.
pub fn hash_json_value<T: Serialize + ?Sized>(value: &T) -> Option<u64> {
    let mut hasher = DefaultHasher::new();
    serde_json::to_writer(HasherWriter::new(&mut hasher), value).ok().map(|_| {
        hasher.write_u8(0xff);
        hasher.finish()
    })
}

/// Hash tool definitions for cache key computation.
pub fn hash_tool_definitions(tools: Option<&[ToolDefinition]>) -> Option<u64> {
    tools.and_then(hash_json_value)
}

/// Compute a stable hash of the system prompt prefix.
///
/// Strips runtime sections (tool catalog, context, active tools) so the hash
/// remains stable across turns even as runtime context changes.
pub fn stable_system_prefix_hash(system_prompt: &str) -> u64 {
    let stable_prefix = system_prompt
        .split("\n## Active Tools\n")
        .next()
        .unwrap_or(system_prompt)
        .split("\n[Runtime Tool Catalog]\n")
        .next()
        .unwrap_or(system_prompt)
        .split("\n[Runtime Context]\n")
        .next()
        .unwrap_or(system_prompt)
        .split("\n[Context]\n")
        .next()
        .unwrap_or(system_prompt)
        .trim_end();
    hash_value(&stable_prefix)
}

/// Generate a deduplication key for low-signal tool attempts.
pub fn low_signal_attempt_key(name: &str, args: &Value) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut input_len = 0usize;
    if serde_json::to_writer(HashingWriter::new(&mut hash, &mut input_len), args).is_err() {
        for byte in b"{}" {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
            input_len = input_len.saturating_add(1);
        }
    }

    format!("{name}:len{input_len}-fnv{hash:016x}")
}

struct HashingWriter<'a> {
    hash: &'a mut u64,
    input_len: &'a mut usize,
}

impl<'a> HashingWriter<'a> {
    fn new(hash: &'a mut u64, input_len: &'a mut usize) -> Self {
        Self { hash, input_len }
    }
}

impl std::io::Write for HashingWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for byte in buf {
            *self.hash ^= u64::from(*byte);
            *self.hash = self.hash.wrapping_mul(0x100000001b3);
            *self.input_len = self.input_len.saturating_add(1);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct HasherWriter<'a, H> {
    hasher: &'a mut H,
}

impl<'a, H> HasherWriter<'a, H> {
    fn new(hasher: &'a mut H) -> Self {
        Self { hasher }
    }
}

impl<H: Hasher> std::io::Write for HasherWriter<'_, H> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.hasher.write(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
