//! Capability-driven reasoning validation before a provider request is sent.

use anyhow::{Result, bail};
use vtcode_config::types::ReasoningEffortLevel;

use crate::provider::LLMProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReasoningEffortMapping {
    pub requested: ReasoningEffortLevel,
    pub effective: ReasoningEffortLevel,
}

impl ReasoningEffortMapping {
    pub fn degraded(self) -> bool {
        self.requested != self.effective
    }
}

pub struct ReasoningEffortMapper;

impl ReasoningEffortMapper {
    pub fn resolve(
        provider: &dyn LLMProvider,
        model: &str,
        requested: ReasoningEffortLevel,
        allow_downgrade: bool,
    ) -> Result<ReasoningEffortMapping> {
        Self::map(requested, provider.supported_reasoning_efforts(model), allow_downgrade)
    }

    pub fn map(
        requested: ReasoningEffortLevel,
        supported: &[&str],
        allow_downgrade: bool,
    ) -> Result<ReasoningEffortMapping> {
        if requested == ReasoningEffortLevel::None || supported.contains(&requested.as_str()) {
            return Ok(ReasoningEffortMapping { requested, effective: requested });
        }
        let ordered_levels = [
            ReasoningEffortLevel::Minimal,
            ReasoningEffortLevel::Low,
            ReasoningEffortLevel::Medium,
            ReasoningEffortLevel::High,
            ReasoningEffortLevel::XHigh,
            ReasoningEffortLevel::Max,
        ];
        if allow_downgrade
            && let Some(position) = ordered_levels.iter().position(|level| *level == requested)
            && let Some(effective) = ordered_levels
                .iter()
                .take(position)
                .rev()
                .find(|level| supported.contains(&level.as_str()))
        {
            return Ok(ReasoningEffortMapping { requested, effective: *effective });
        }
        bail!(
            "Requested reasoning effort `{requested}` is unsupported by this route (supported: {}). Select a supported effort or explicitly enable agent.allow_reasoning_effort_downgrade",
            supported.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_matrix_never_silently_loses_fidelity() {
        for supported in [
            &["low", "medium", "high", "xhigh", "max"][..],
            &["low", "medium", "high", "max"][..],
            &["minimal", "low", "medium", "high"][..],
        ] {
            for requested in [
                ReasoningEffortLevel::None,
                ReasoningEffortLevel::Minimal,
                ReasoningEffortLevel::Low,
                ReasoningEffortLevel::Medium,
                ReasoningEffortLevel::High,
                ReasoningEffortLevel::XHigh,
                ReasoningEffortLevel::Max,
                ReasoningEffortLevel::Unknown,
            ] {
                let strict = ReasoningEffortMapper::map(requested, supported, false);
                assert_eq!(
                    strict.is_ok(),
                    requested == ReasoningEffortLevel::None || supported.contains(&requested.as_str())
                );
                if let Ok(mapping) = strict {
                    assert!(!mapping.degraded());
                }
            }
        }
        assert_eq!(
            ReasoningEffortMapper::map(ReasoningEffortLevel::Max, &["high", "xhigh"], true)
                .unwrap()
                .effective,
            ReasoningEffortLevel::XHigh
        );
        assert_eq!(
            ReasoningEffortMapper::map(ReasoningEffortLevel::Max, &["high"], true)
                .unwrap()
                .effective,
            ReasoningEffortLevel::High
        );
        assert!(ReasoningEffortMapper::map(ReasoningEffortLevel::Unknown, &["high"], true).is_err());
    }
}
