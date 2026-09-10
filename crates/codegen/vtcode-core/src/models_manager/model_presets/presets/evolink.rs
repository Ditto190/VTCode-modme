//! evolink_presets — provider preset definitions for evolink.

use super::super::ModelPreset;
use super::reasoning_preset;
use crate::config::constants::models::evolink as evolink_models;
use crate::config::models::Provider;
use crate::config::types::ReasoningEffortLevel;
pub(crate) fn evolink_presets() -> Vec<ModelPreset> {
    vec![
        ModelPreset {
            id: "evolink/gemini-3.1-pro-preview".to_string(),
            model: evolink_models::GEMINI_3_1_PRO.to_string(),
            display_name: "Gemini 3.1 Pro (Evolink)".to_string(),
            description: "Gemini 3.1 Pro served through the Evolink gateway via OpenAI SDK format.".to_string(),
            provider: Provider::Evolink,
            default_reasoning_effort: ReasoningEffortLevel::Medium,
            supported_reasoning_efforts: vec![reasoning_preset(ReasoningEffortLevel::Medium, "Balanced")],
            is_default: false,
            upgrade: None,
            show_in_picker: true,
            supported_in_api: true,
            context_window: Some(1_000_000),
        },
        ModelPreset {
            id: "evolink/gemini-3.5-flash".to_string(),
            model: evolink_models::GEMINI_3_7_FLASH.to_string(),
            display_name: "Gemini 3.5 Flash (Evolink)".to_string(),
            description: "Gemini 3.5 Flash served through the Evolink gateway via OpenAI SDK format.".to_string(),
            provider: Provider::Evolink,
            default_reasoning_effort: ReasoningEffortLevel::Medium,
            supported_reasoning_efforts: vec![reasoning_preset(ReasoningEffortLevel::Medium, "Balanced")],
            is_default: false,
            upgrade: None,
            show_in_picker: true,
            supported_in_api: true,
            context_window: Some(1_000_000),
        },
        ModelPreset {
            id: "evolink/MiniMax-M3".to_string(),
            model: evolink_models::MINIMAX_M3.to_string(),
            display_name: "MiniMax-M3 (Evolink)".to_string(),
            description: "MiniMax-M3 frontier multimodal model served through the Evolink gateway.".to_string(),
            provider: Provider::Evolink,
            default_reasoning_effort: ReasoningEffortLevel::Medium,
            supported_reasoning_efforts: vec![reasoning_preset(ReasoningEffortLevel::Medium, "Balanced")],
            is_default: false,
            upgrade: None,
            show_in_picker: true,
            supported_in_api: true,
            context_window: Some(1_000_000),
        },
        ModelPreset {
            id: "evolink/claude-haiku-4-5-20251001".to_string(),
            model: evolink_models::CLAUDE_SONNET_5.to_string(),
            display_name: "Claude Haiku 4.5 (Evolink)".to_string(),
            description: "Claude Haiku 4.5 fast model served through Evolink via Anthropic Messages API.".to_string(),
            provider: Provider::Evolink,
            default_reasoning_effort: ReasoningEffortLevel::Low,
            supported_reasoning_efforts: vec![
                reasoning_preset(ReasoningEffortLevel::Low, "Fast"),
                reasoning_preset(ReasoningEffortLevel::Medium, "Balanced"),
            ],
            is_default: false,
            upgrade: None,
            show_in_picker: true,
            supported_in_api: true,
            context_window: Some(200_000),
        },
    ]
}
