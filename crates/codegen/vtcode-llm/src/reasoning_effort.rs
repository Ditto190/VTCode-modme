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

    /// Best-effort counterpart to [`Self::resolve`] for session-persistent
    /// config.
    ///
    /// A configured effort is durable state: when the active route does not
    /// support it, the effort is omitted for this request (the provider keeps
    /// its own default) instead of aborting request assembly on every turn.
    #[must_use]
    pub fn resolve_or_omit(
        provider: &dyn LLMProvider,
        model: &str,
        requested: ReasoningEffortLevel,
        allow_downgrade: bool,
    ) -> Option<ReasoningEffortMapping> {
        Self::map_or_omit(requested, provider.supported_reasoning_efforts(model), allow_downgrade)
    }

    /// Route-free variant of [`Self::resolve_or_omit`] for callers that
    /// already hold the supported-level list.
    #[must_use]
    pub(crate) fn map_or_omit(
        requested: ReasoningEffortLevel,
        supported: &[&str],
        allow_downgrade: bool,
    ) -> Option<ReasoningEffortMapping> {
        match Self::map(requested, supported, allow_downgrade) {
            Ok(mapping) => Some(mapping),
            Err(error) => {
                tracing::warn!(
                    requested = %requested,
                    error = %error,
                    "Configured reasoning effort is unsupported on this route; omitting it for this request"
                );
                None
            }
        }
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

    #[test]
    fn lenient_resolution_omits_unsupported_effort_instead_of_failing() {
        // A route without reasoning support cannot host any effort: omit, warn,
        // and let the request proceed instead of failing every turn.
        assert_eq!(ReasoningEffortMapper::map_or_omit(ReasoningEffortLevel::Max, &[], false), None);
        assert_eq!(ReasoningEffortMapper::map_or_omit(ReasoningEffortLevel::Unknown, &["low", "high"], false), None);
        // A supported request passes through untouched.
        assert_eq!(
            ReasoningEffortMapper::map_or_omit(ReasoningEffortLevel::High, &["low", "high"], false)
                .map(|mapping| mapping.effective),
            Some(ReasoningEffortLevel::High)
        );
    }
}
