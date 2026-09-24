//! Server-Sent Events (SSE) stream decoder for Anthropic Claude API
//!
//! Handles streaming responses from the Anthropic API, decoding SSE events
//! and accumulating partial content into a complete LLMResponse.

use crate::provider::LLMError;
use crate::provider::LLMStreamEvent;
use crate::providers::anthropic_types::{
    AnthropicContentBlock, AnthropicStreamDelta, AnthropicStreamEvent, CacheControl,
};
use crate::providers::error_handling::format_network_error;
use crate::providers::shared;

use async_stream::try_stream;
use futures::StreamExt;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

use super::block_order::StreamBlockOrder;
use super::response_parser::{parse_finish_reason, parse_usage, stop_details_reasoning_detail};

enum ReasoningBlockState {
    Thinking {
        thinking: String,
        signature: Option<String>,
    },
    Redacted {
        data: String,
    },
    Compaction {
        content: Option<String>,
        signature: Option<String>,
        cache_control: Option<CacheControl>,
        extra: Map<String, Value>,
    },
}

pub fn create_stream(
    response: reqwest::Response,
    model: String,
    request_id: Option<String>,
    organization_id: Option<String>,
) -> crate::provider::LLMStream {
    let stream = try_stream! {
        let mut body_stream = response.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut offset = 0usize;
        let mut decoder = shared::Utf8StreamDecoder::new();
        let mut aggregator = shared::StreamAggregator::new(model);
        let mut reasoning_blocks = BTreeMap::new();
        let mut finalized_reasoning_details = Vec::new();
        let mut advisor_blocks: Vec<Value> = Vec::new();
        let mut stop_details_detail: Option<String> = None;
        let mut block_order = StreamBlockOrder::default();

        while let Some(chunk_result) = body_stream.next().await {
            let chunk = chunk_result.map_err(|err| {
                format_network_error("Anthropic", &anyhow::Error::new(err))
            })?;

            decoder.push_bytes(&chunk, &mut buf);

            while let Some((split_idx, delimiter_len)) = shared::find_sse_boundary_bytes(&buf, offset) {
                let event = std::str::from_utf8(&buf[offset..split_idx]).expect("valid utf-8 stream data");
                offset = split_idx + delimiter_len;

                if let Some(data_payload) = shared::extract_data_payload(event) {
                    let trimmed_payload = data_payload.trim();
                    if trimmed_payload.is_empty() {
                        continue;
                    }

                    let event: AnthropicStreamEvent = serde_json::from_str(trimmed_payload).map_err(|err| {
                        LLMError::Provider {
                            message: format!("Failed to parse stream event: {err}"),
                            metadata: None,
                        }
                    })?;

                    block_order.observe(&event);
                    match event {
                        AnthropicStreamEvent::MessageStart { message } => {
                            let usage_value = serde_json::to_value(&message.usage).unwrap_or_else(|_| Value::Object(Map::new()));
                            aggregator.set_usage(parse_usage(&usage_value));
                        }
                        AnthropicStreamEvent::ContentBlockStart {
                            index,
                            content_block:
                                AnthropicContentBlock::Thinking {
                                    thinking,
                                    signature,
                                    ..
                                },
                        } => {
                            reasoning_blocks.insert(index, ReasoningBlockState::Thinking {
                                thinking,
                                signature,
                            });
                        }
                        AnthropicStreamEvent::ContentBlockStart {
                            index,
                            content_block: AnthropicContentBlock::RedactedThinking { data, .. },
                        } => {
                            reasoning_blocks.insert(index, ReasoningBlockState::Redacted { data });
                        }
                        AnthropicStreamEvent::ContentBlockStart {
                            index,
                            content_block:
                                AnthropicContentBlock::Compaction {
                                    content,
                                    signature,
                                    cache_control,
                                    extra,
                                },
                        } => {
                            if let Some(summary) = content.as_ref() {
                                aggregator.compaction = Some(summary.clone());
                            }
                            reasoning_blocks.insert(
                                index,
                                ReasoningBlockState::Compaction {
                                    content,
                                    signature,
                                    cache_control,
                                    extra,
                                },
                            );
                        }
                        AnthropicStreamEvent::ContentBlockStart {
                            index,
                            content_block: AnthropicContentBlock::ToolUse(tool_use),
                        } => {
                            if aggregator.tool_builders.len() <= index {
                                aggregator.tool_builders.resize_with(index + 1, shared::ToolCallBuilder::default);
                            }
                            let mut delta = Map::new();
                            delta.insert("id".to_string(), Value::String(tool_use.id));
                            let mut func = Map::new();
                            func.insert("name".to_string(), Value::String(tool_use.name));
                            delta.insert("function".to_string(), Value::Object(func));
                            aggregator.tool_builders[index].apply_delta(&Value::Object(delta));
                        }
                        AnthropicStreamEvent::ContentBlockStart {
                            content_block:
                                AnthropicContentBlock::ServerToolUse { id, name, input },
                            ..
                        } => {
                            if name == "advisor" {
                                advisor_blocks.push(serde_json::json!({
                                    "type": "server_tool_use",
                                    "id": id,
                                    "name": name,
                                    "input": input,
                                }));
                            }
                        }
                        AnthropicStreamEvent::ContentBlockStart {
                            content_block:
                                AnthropicContentBlock::AdvisorToolResult { tool_use_id, content },
                            ..
                        } => {
                            advisor_blocks.push(serde_json::json!({
                                "type": "advisor_tool_result",
                                "tool_use_id": tool_use_id,
                                "content": content,
                            }));
                        }
                        AnthropicStreamEvent::ContentBlockDelta { index, delta } => {
                            match delta {
                                AnthropicStreamDelta::TextDelta { text } => {
                                    for event in aggregator.handle_content(&text) {
                                        yield event;
                                    }
                                }
                                AnthropicStreamDelta::ThinkingDelta { thinking } => {
                                    match reasoning_blocks.entry(index) {
                                        std::collections::btree_map::Entry::Occupied(mut entry) => {
                                            if let ReasoningBlockState::Thinking { thinking: accumulated, .. } = entry.get_mut() {
                                                accumulated.push_str(&thinking);
                                            }
                                        }
                                        std::collections::btree_map::Entry::Vacant(entry) => {
                                            entry.insert(ReasoningBlockState::Thinking {
                                                thinking: thinking.clone(),
                                                signature: None,
                                            });
                                        }
                                    }
                                    if let Some(delta) = aggregator.handle_reasoning(&thinking) {
                                        yield LLMStreamEvent::Reasoning { delta };
                                    }
                                }
                                AnthropicStreamDelta::SignatureDelta { signature } => {
                                    match reasoning_blocks.entry(index) {
                                        std::collections::btree_map::Entry::Occupied(mut entry) => {
                                            if let ReasoningBlockState::Thinking { signature: current, .. } = entry.get_mut() {
                                                *current = Some(signature.clone());
                                            }
                                        }
                                        std::collections::btree_map::Entry::Vacant(entry) => {
                                            entry.insert(ReasoningBlockState::Thinking {
                                                thinking: String::new(),
                                                signature: Some(signature.clone()),
                                            });
                                        }
                                    }
                                    yield LLMStreamEvent::ReasoningSignature { signature };
                                }
                                AnthropicStreamDelta::CompactionDelta { content, extra } => {
                                    if let Some(content) = content.as_ref() {
                                        if let Some(summary) = aggregator.compaction.as_mut() {
                                            summary.push_str(content);
                                        } else {
                                            aggregator.compaction = Some(content.clone());
                                        }
                                    }
                                    match reasoning_blocks.entry(index) {
                                        std::collections::btree_map::Entry::Occupied(mut entry) => {
                                            if let ReasoningBlockState::Compaction {
                                                content: current,
                                                extra: state_extra,
                                                ..
                                            } = entry.get_mut()
                                            {
                                                if let Some(content) = content.as_ref() {
                                                    if let Some(accumulated) = current.as_mut() {
                                                        accumulated.push_str(content);
                                                    } else {
                                                        *current = Some(content.clone());
                                                    }
                                                }
                                                state_extra.extend(extra);
                                            }
                                        }
                                        std::collections::btree_map::Entry::Vacant(entry) => {
                                            entry.insert(ReasoningBlockState::Compaction {
                                                content,
                                                signature: None,
                                                cache_control: None,
                                                extra,
                                            });
                                        }
                                    }
                                }
                                AnthropicStreamDelta::InputJsonDelta { partial_json } => {
                                    if aggregator.tool_builders.len() <= index {
                                        aggregator.tool_builders.resize_with(index + 1, shared::ToolCallBuilder::default);
                                    }
                                    let mut delta_map = Map::new();
                                    let mut func = Map::new();
                                    func.insert("arguments".to_string(), Value::String(partial_json));
                                    delta_map.insert("function".to_string(), Value::Object(func));
                                    aggregator.tool_builders[index].apply_delta(&Value::Object(delta_map));
                                }
                                AnthropicStreamDelta::Unknown => {}
                            }
                        }
                        AnthropicStreamEvent::ContentBlockStop { index } => {
                            if let Some(reasoning_block) = reasoning_blocks.remove(&index) {
                                finalized_reasoning_details.push(serialize_reasoning_block_detail(reasoning_block));
                            }
                        }
                        AnthropicStreamEvent::MessageDelta { delta, usage } => {
                            if let Some(u) = usage {
                                let usage_value =
                                    serde_json::to_value(&u).unwrap_or_else(|_| Value::Object(Map::new()));
                                let delta_usage = parse_usage(&usage_value);
                                let mut current_usage = aggregator.usage.take().unwrap_or_default();

                                // `message_delta.usage` may contain only the
                                // output count. Keep the message-start input
                                // and cache metrics when they are omitted.
                                if delta_usage.prompt_tokens != 0 {
                                    current_usage.prompt_tokens = delta_usage.prompt_tokens;
                                }
                                current_usage.completion_tokens = delta_usage.completion_tokens;
                                current_usage.total_tokens = current_usage
                                    .prompt_tokens
                                    .saturating_add(current_usage.completion_tokens);
                                if delta_usage.cached_prompt_tokens.is_some() {
                                    current_usage.cached_prompt_tokens = delta_usage.cached_prompt_tokens;
                                    current_usage.cache_read_tokens = delta_usage.cache_read_tokens;
                                }
                                if delta_usage.cache_creation_tokens.is_some() {
                                    current_usage.cache_creation_tokens = delta_usage.cache_creation_tokens;
                                }
                                if delta_usage.iterations.is_some() {
                                    current_usage.iterations = delta_usage.iterations;
                                }
                                aggregator.usage = Some(current_usage);
                            }
                            if let Some(reason) = delta.stop_reason {
                                aggregator.set_finish_reason(parse_finish_reason(&reason));
                            }
                            if let Some(detail) = delta.stop_details.as_ref().and_then(stop_details_reasoning_detail) {
                                stop_details_detail = Some(detail);
                            }
                        }
                        AnthropicStreamEvent::Error { error } => {
                            Err(LLMError::Provider {
                                message: error.message,
                                metadata: None,
                            })?
                        }
                        _ => {}
                    }
                }
            }

            // Drain the consumed prefix so `buf` stays bounded to the
            // unprocessed tail rather than growing for the entire stream.
            if offset > 0 {
                buf.drain(..offset);
                offset = 0;
            }
        }

        for (_, reasoning_block) in reasoning_blocks {
            finalized_reasoning_details.push(serialize_reasoning_block_detail(reasoning_block));
        }

        // Same order as the non-streaming parser: reasoning blocks, then
        // stop_details, then advisor blocks.
        finalized_reasoning_details.extend(stop_details_detail);

        let mut response = aggregator.finalize();
        if !finalized_reasoning_details.is_empty() {
            response.reasoning_details = Some(finalized_reasoning_details);
        }
        if !advisor_blocks.is_empty() {
            let detail = serde_json::json!({
                "type": "advisor",
                "blocks": advisor_blocks,
            });
            let mut details = response.reasoning_details.unwrap_or_default();
            details.push(detail.to_string());
            response.reasoning_details = Some(details);
        }
        if let Some(detail) = block_order.into_detail() {
            response.reasoning_details.get_or_insert_with(Vec::new).push(detail);
        }
        response.request_id = request_id.clone();
        response.organization_id = organization_id.clone();

        yield LLMStreamEvent::Completed { response: Box::new(response) };
    };

    Box::pin(stream)
}

fn serialize_reasoning_block_detail(block: ReasoningBlockState) -> String {
    match block {
        ReasoningBlockState::Thinking { thinking, signature } => {
            let mut detail = serde_json::json!({
                "type": "thinking",
                "thinking": thinking,
            });
            if let Some(signature) = signature
                && let Some(obj) = detail.as_object_mut()
            {
                obj.insert("signature".to_string(), Value::String(signature));
            }
            detail.to_string()
        }
        ReasoningBlockState::Redacted { data } => serde_json::json!({
            "type": "redacted_thinking",
            "data": data,
        })
        .to_string(),
        ReasoningBlockState::Compaction { content, signature, cache_control, extra } => {
            serde_json::to_string(&AnthropicContentBlock::Compaction { content, signature, cache_control, extra })
                .unwrap_or_else(|_| "{\"type\":\"compaction\",\"content\":null}".to_string())
        }
    }
}
