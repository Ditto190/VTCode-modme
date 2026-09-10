//! deepseek_presets — provider preset definitions for deepseek.

use super::super::{ModelPreset, ReasoningEffortPreset};
use crate::config::models::Provider;
use crate::config::types::ReasoningEffortLevel;
pub(crate) fn deepseek_presets() -> Vec<ModelPreset> {
    vec![ModelPreset {
        id: "deepseek-v4.1-flash".to_string(),
        model: "deepseek-v4.1-flash".to_string(),
        display_name: "DeepSeek V4.1 Flash".to_string(),
        description: "Latest DeepSeek flash model with improved reasoning, efficiency, and agent capabilities"
            .to_string(),
        provider: Provider::DeepSeek,
        default_reasoning_effort: ReasoningEffortLevel::High,
        supported_reasoning_efforts: vec![
            ReasoningEffortPreset {
                effort: ReasoningEffortLevel::Low,
                description: "Fast, light reasoning".to_string(),
            },
            ReasoningEffortPreset {
                effort: ReasoningEffortLevel::High,
                description: "Balanced".to_string(),
            },
            ReasoningEffortPreset {
                effort: ReasoningEffortLevel::Max,
                description: "Maximum thinking".to_string(),
            },
        ],
        is_default: true,
        upgrade: None,
        show_in_picker: true,
        supported_in_api: true,
        context_window: Some(1_000_000),
    }]
}
