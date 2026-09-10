//! deepseek_presets — provider preset definitions for deepseek.

use super::super::{ModelPreset, ModelUpgrade, ReasoningEffortPreset};
use crate::config::models::Provider;
use crate::config::types::ReasoningEffortLevel;
pub(crate) fn deepseek_presets() -> Vec<ModelPreset> {
    vec![
        ModelPreset {
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
        },
        ModelPreset {
            id: "deepseek-v4-pro".to_string(),
            model: "deepseek-v4-pro".to_string(),
            display_name: "DeepSeek V4 Pro".to_string(),
            description: "High-performance reasoning model with advanced thinking capabilities".to_string(),
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
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "deepseek-v4.1-flash".to_string(),
                migration_config_key: String::new(),
                upgrade_copy: Some("Upgrade to DeepSeek V4.1 Flash for better performance and lower cost.".to_string()),
                reasoning_effort_mapping: None,
                model_link: None,
            }),
            show_in_picker: true,
            supported_in_api: true,
            context_window: Some(1_000_000),
        },
        ModelPreset {
            id: "deepseek-v4-flash".to_string(),
            model: "deepseek-v4-flash".to_string(),
            display_name: "DeepSeek V4 Flash".to_string(),
            description: "Official DeepSeek V4 Flash release with enhanced agent capabilities for coding and tool use"
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
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "deepseek-v4.1-flash".to_string(),
                migration_config_key: String::new(),
                upgrade_copy: Some("Upgrade to DeepSeek V4.1 Flash — V4 Flash is now retired.".to_string()),
                reasoning_effort_mapping: None,
                model_link: None,
            }),
            show_in_picker: true,
            supported_in_api: true,
            context_window: Some(1_000_000),
        },
    ]
}
