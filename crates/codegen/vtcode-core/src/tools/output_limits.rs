//! Shared model-visible tool-output preview limits.

use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use vtcode_utility_tool_specs::{
    DEFAULT_MAX_OUTPUT_TOKENS, MAX_MAX_OUTPUT_TOKENS, MAX_OUTPUT_TOKENS_FIELD, MIN_MAX_OUTPUT_TOKENS,
};

pub(crate) const OUTPUT_PREVIEW_CHARS_PER_TOKEN: usize = 4;

/// Default per-result preview budget for plan-mode inspections that omit an
/// explicit `max_output_tokens` (`2_000` tokens ≈ 8 KiB). Planning research
/// fans out across many reads, so the smaller default keeps a dozen previews
/// inside the `96 KiB` plan turn budget. Explicit caller values and
/// verification commands are never clamped.
pub(crate) const PLAN_MODE_DEFAULT_MAX_OUTPUT_TOKENS: usize = 2_000;

/// Validates and returns the requested model-visible result preview budget.
///
/// A missing value resolves to the stable default. Values must be JSON integers
/// so callers cannot silently coerce floats or strings into a larger context
/// allocation than the model requested.
pub(crate) fn max_output_tokens(args: &Value) -> Result<usize> {
    resolve_max_output_tokens(args, false, false)
}

/// Planning-aware variant of [`max_output_tokens`].
///
/// An explicitly provided integer keeps existing validation exactly;
/// verification commands keep the full default so build/test output stays
/// authoritative. Only an omitted value on a non-verification call while
/// planning is active resolves to [`PLAN_MODE_DEFAULT_MAX_OUTPUT_TOKENS`].
pub(crate) fn resolve_max_output_tokens(args: &Value, planning_active: bool, is_verification: bool) -> Result<usize> {
    if args.get(MAX_OUTPUT_TOKENS_FIELD).is_none() && planning_active && !is_verification {
        return Ok(PLAN_MODE_DEFAULT_MAX_OUTPUT_TOKENS);
    }
    max_output_tokens_uncapped(args)
}

fn max_output_tokens_uncapped(args: &Value) -> Result<usize> {
    let Some(value) = args.get(MAX_OUTPUT_TOKENS_FIELD) else {
        return Ok(DEFAULT_MAX_OUTPUT_TOKENS);
    };
    let Some(tokens) = value.as_u64() else {
        return Err(anyhow!(
            "max_output_tokens must be an integer between {MIN_MAX_OUTPUT_TOKENS} and {MAX_MAX_OUTPUT_TOKENS}"
        ));
    };
    let tokens = usize::try_from(tokens).with_context(|| {
        format!("max_output_tokens must be an integer between {MIN_MAX_OUTPUT_TOKENS} and {MAX_MAX_OUTPUT_TOKENS}")
    })?;
    if !(MIN_MAX_OUTPUT_TOKENS..=MAX_MAX_OUTPUT_TOKENS).contains(&tokens) {
        return Err(anyhow!(
            "max_output_tokens must be an integer between {MIN_MAX_OUTPUT_TOKENS} and {MAX_MAX_OUTPUT_TOKENS}"
        ));
    }
    Ok(tokens)
}

/// Removes execution-only output metadata before validating a legacy schema.
/// The field is retained in the original payload and reaches the execution
/// pipeline, but schemas authored before this common option remain strict.
pub(crate) fn args_without_output_metadata(args: &Value) -> Value {
    let mut args = args.clone();
    if let Some(object) = args.as_object_mut() {
        object.remove(MAX_OUTPUT_TOKENS_FIELD);
    }
    args
}

/// Whether a handler explicitly consumes the common output-limit field.
pub(crate) fn handler_accepts_output_metadata(schema: Option<&Value>) -> bool {
    schema
        .and_then(|schema| schema.get("properties"))
        .and_then(Value::as_object)
        .is_some_and(|properties| properties.contains_key(MAX_OUTPUT_TOKENS_FIELD))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn max_output_tokens_defaults_and_enforces_integer_bounds() {
        assert_eq!(max_output_tokens(&json!({})).unwrap(), DEFAULT_MAX_OUTPUT_TOKENS);
        assert_eq!(max_output_tokens(&json!({"max_output_tokens": 1})).unwrap(), 1);
        assert_eq!(max_output_tokens(&json!({"max_output_tokens": 50_000})).unwrap(), 50_000);
        assert!(max_output_tokens(&json!({"max_output_tokens": 0})).is_err());
        assert!(max_output_tokens(&json!({"max_output_tokens": 50_001})).is_err());
        assert!(max_output_tokens(&json!({"max_output_tokens": 1.0})).is_err());
        assert!(max_output_tokens(&json!({"max_output_tokens": "100"})).is_err());
    }

    #[test]
    fn plan_mode_clamp_applies_only_to_omitted_non_verification_defaults() {
        // Omitted value in planning resolves to the smaller inspection default.
        assert_eq!(resolve_max_output_tokens(&json!({}), true, false).unwrap(), PLAN_MODE_DEFAULT_MAX_OUTPUT_TOKENS);
        // The plan default resolves strictly below the execution default.
        assert!(
            resolve_max_output_tokens(&json!({}), true, false).unwrap()
                < resolve_max_output_tokens(&json!({}), false, false).unwrap()
        );
        // Explicit caller values are never clamped, even in planning.
        assert_eq!(
            resolve_max_output_tokens(&json!({"max_output_tokens": DEFAULT_MAX_OUTPUT_TOKENS}), true, false).unwrap(),
            DEFAULT_MAX_OUTPUT_TOKENS
        );
        assert_eq!(resolve_max_output_tokens(&json!({"max_output_tokens": 1}), true, false).unwrap(), 1);
        // Verification commands keep the full default in planning.
        assert_eq!(resolve_max_output_tokens(&json!({}), true, true).unwrap(), DEFAULT_MAX_OUTPUT_TOKENS);
        // Execution mode is untouched.
        assert_eq!(resolve_max_output_tokens(&json!({}), false, false).unwrap(), DEFAULT_MAX_OUTPUT_TOKENS);
        // Invalid values still error in every mode.
        assert!(resolve_max_output_tokens(&json!({"max_output_tokens": 0}), true, false).is_err());
        assert!(resolve_max_output_tokens(&json!({"max_output_tokens": "100"}), true, false).is_err());
    }

    #[test]
    fn output_metadata_is_removed_only_from_schema_validation_copy() {
        let original = json!({"input": "value", "max_output_tokens": 23});
        assert_eq!(args_without_output_metadata(&original), json!({"input": "value"}));
        assert_eq!(original["max_output_tokens"], 23);
    }

    #[test]
    fn handler_metadata_forwarding_requires_an_explicit_schema_field() {
        assert!(!handler_accepts_output_metadata(Some(&json!({"type": "object"}))));
        assert!(handler_accepts_output_metadata(Some(&json!({
            "type": "object",
            "properties": {"max_output_tokens": {"type": "integer"}}
        }))));
    }
}
