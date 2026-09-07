use crate::task::EvalTask;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSuite {
    pub(crate) id: String,
    pub name: String,
    pub tasks: Vec<EvalTask>,
    pub attempts: u32,
}

impl EvalSuite {
    /// Validate suite invariants shared by CLI and programmatic callers.
    ///
    /// Serde intentionally still accepts `attempts: 0` so persisted fixtures
    /// fail with this explicit error instead of a schema break.
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.attempts >= 1, "evaluation suite attempts must be at least one");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::EvalCategory;

    #[test]
    fn suite_round_trips_through_json() {
        let suite = EvalSuite {
            id: "s1".into(),
            name: "demo".into(),
            tasks: vec![crate::task::EvalTask {
                id: "t1".into(),
                name: "t1".into(),
                category: EvalCategory::Capability,
                prompt: "do it".into(),
                verify_commands: vec!["cargo test".into()],
                timeout_secs: Some(30),
            }],
            attempts: 3,
        };
        let json = serde_json::to_string(&suite).unwrap();
        let back: EvalSuite = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "s1");
        assert_eq!(back.attempts, 3);
        assert_eq!(back.tasks.len(), 1);
        assert_eq!(back.tasks[0].timeout_secs, Some(30));
    }

    #[test]
    fn suite_rejects_zero_attempts_via_validation() {
        // Serde itself allows 0; `validate()` is the shared guardrail called
        // by both the CLI entrypoint and `run_suite_with_options`.
        let suite: EvalSuite = serde_json::from_str(r#"{"id":"s","name":"n","tasks":[],"attempts":0}"#).unwrap();
        assert_eq!(suite.attempts, 0);
        assert!(suite.validate().is_err());
        let valid: EvalSuite = serde_json::from_str(r#"{"id":"s","name":"n","tasks":[],"attempts":1}"#).unwrap();
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn preview_budget_blocked_replan_regression_suite_is_env_verified() {
        let json = include_str!("../evals/preview-budget-blocked-replan.json");
        let suite: EvalSuite = serde_json::from_str(json).unwrap();
        assert_eq!(suite.id, "preview-budget-blocked-replan");
        assert_eq!(suite.attempts, 3);
        assert_eq!(suite.tasks.len(), 3);
        for task in &suite.tasks {
            assert_eq!(task.category, EvalCategory::Regression);
            assert!(!task.prompt.is_empty());
            assert!(!task.verify_commands.is_empty());
        }
        let ids: Vec<&str> = suite.tasks.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "preview-budget-no-retry",
                "blocked-handoff-resume",
                "replan-continuation"
            ]
        );
    }
}
