//! Explicit global activity states shared by the runloop and terminal UI.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActivityState {
    #[default]
    Idle,
    /// The agent is working inside the plan workflow (research/synthesis).
    Planning,
    PreparingFreshExecutionThread,
    RestoringApprovedPlan,
    /// The agent is executing an approved plan.
    Building,
    StartingBuild,
    /// A bounded tool-free recovery pass is running after blocked tool calls.
    /// Tools are disabled for this pass; input stays enabled.
    Recovery,
    /// The turn is blocked and waiting for user guidance (`continue`, new
    /// instructions, or `vtcode --resume`). Distinct from `Idle` so the TUI
    /// can keep a persistent blocked hint visible.
    Blocked,
}

impl ActivityState {
    /// Transient handoff states that must block input and mode switches.
    pub const fn is_busy(self) -> bool {
        matches!(self, Self::PreparingFreshExecutionThread | Self::RestoringApprovedPlan | Self::StartingBuild)
    }

    /// Whether primary-agent/mode switching must be rejected while this state
    /// is visible. Long-lived Build, Recovery, and Blocked states keep input
    /// enabled, but their active turn state still owns the mode boundary.
    pub const fn locks_mode_switch(self) -> bool {
        matches!(
            self,
            Self::PreparingFreshExecutionThread
                | Self::RestoringApprovedPlan
                | Self::Building
                | Self::StartingBuild
                | Self::Recovery
                | Self::Blocked
        )
    }

    /// Long-lived display stages that keep input enabled between turns.
    pub const fn is_stage(self) -> bool {
        matches!(self, Self::Planning | Self::Building | Self::Recovery)
    }

    pub const fn status(self) -> Option<&'static str> {
        match self {
            Self::Idle => None,
            Self::Planning => Some("Planning..."),
            Self::PreparingFreshExecutionThread => Some("Preparing fresh execution thread..."),
            Self::RestoringApprovedPlan => Some("Restoring approved plan..."),
            Self::Building => Some("Building..."),
            Self::StartingBuild => Some("Starting build..."),
            Self::Recovery => Some("Recovery: tools disabled..."),
            Self::Blocked => Some("Blocked — Type 'continue' to retry..."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ActivityState;

    #[test]
    fn idle_has_no_status_and_is_not_busy_or_stage() {
        assert_eq!(ActivityState::Idle.status(), None);
        assert!(!ActivityState::Idle.is_busy());
        assert!(!ActivityState::Idle.is_stage());
    }

    #[test]
    fn stages_are_not_busy_but_keep_a_status() {
        for state in [ActivityState::Planning, ActivityState::Building] {
            assert!(!state.is_busy());
            assert!(state.is_stage());
            assert!(state.status().is_some());
        }
        assert_eq!(ActivityState::Planning.status(), Some("Planning..."));
        assert_eq!(ActivityState::Building.status(), Some("Building..."));
    }

    #[test]
    fn transient_handoffs_are_busy_but_not_stages() {
        for state in [
            ActivityState::PreparingFreshExecutionThread,
            ActivityState::RestoringApprovedPlan,
            ActivityState::StartingBuild,
        ] {
            assert!(state.is_busy());
            assert!(!state.is_stage());
            assert!(state.status().is_some());
        }
    }

    #[test]
    fn mode_switch_is_locked_for_active_build_recovery_and_blocked_states() {
        for state in [
            ActivityState::PreparingFreshExecutionThread,
            ActivityState::RestoringApprovedPlan,
            ActivityState::Building,
            ActivityState::StartingBuild,
            ActivityState::Recovery,
            ActivityState::Blocked,
        ] {
            assert!(state.locks_mode_switch(), "{state:?} must lock mode switching");
        }
        assert!(!ActivityState::Idle.locks_mode_switch());
        assert!(!ActivityState::Planning.locks_mode_switch());
    }
}
