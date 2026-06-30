#![allow(unused_imports)]

#[allow(unused_imports)]
use super::*;

pub(super) fn build_atomic_rule_yaml(request: &StructuralSearchRequest, lang: &str) -> String {
    use std::fmt::Write as _;
    let mut yaml = String::new();
    let _ = writeln!(yaml, "id: atomic-count");
    let _ = writeln!(yaml, "language: {lang}");
    let _ = writeln!(yaml, "severity: info");

    // Emit local utility rules if present.
    if let Some(utils) = &request.utils
        && !utils.is_empty()
    {
        yaml.push_str("utils:\n");
        for (util_name, util_rule) in utils {
            let _ = writeln!(yaml, "  {util_name}:");
            value_to_yaml(&mut yaml, util_rule, 4);
        }
    }

    let _ = writeln!(yaml, "rule:");

    if let Some(pattern) = request.pattern() {
        let _ = writeln!(yaml, "  pattern: {}", yaml_escape_scalar(pattern));
    }
    if let Some(kind) = request.kind() {
        let _ = writeln!(yaml, "  kind: {}", yaml_escape_scalar(kind));
    }
    if let Some(regex) = request.regex_pattern() {
        let _ = writeln!(yaml, "  regex: {}", yaml_escape_scalar(regex));
    }
    if let Some(selector) = request.selector.as_deref().filter(|s| !s.trim().is_empty()) {
        let _ = writeln!(yaml, "  selector: {}", yaml_escape_scalar(selector));
    }
    if let Some(strictness) = &request.strictness {
        let _ = writeln!(yaml, "  strictness: {}", strictness.as_str());
    }

    if let Some(nth) = &request.nth_child {
        match nth {
            NthChildInput::Number(n) => {
                let _ = writeln!(yaml, "  nthChild: {n}");
            }
            NthChildInput::Formula(f) => {
                let _ = writeln!(yaml, "  nthChild: {}", yaml_escape_scalar(f));
            }
            NthChildInput::Object(obj) => {
                let _ = writeln!(yaml, "  nthChild:");
                match &obj.position {
                    Value::Number(n) => {
                        let _ = writeln!(yaml, "    position: {n}");
                    }
                    Value::String(s) => {
                        let _ = writeln!(yaml, "    position: {}", yaml_escape_scalar(s));
                    }
                    _ => {
                        let _ = writeln!(yaml, "    position: {}", obj.position);
                    }
                }
                if let Some(reverse) = obj.reverse {
                    let _ = writeln!(yaml, "    reverse: {reverse}");
                }
                if let Some(of_rule) = &obj.of_rule {
                    let _ = writeln!(yaml, "    ofRule:");
                    if let Some(of_obj) = of_rule.as_object() {
                        for (k, v) in of_obj {
                            match v {
                                Value::String(s) => {
                                    let _ = writeln!(yaml, "      {k}: {}", yaml_escape_scalar(s));
                                }
                                Value::Number(n) => {
                                    let _ = writeln!(yaml, "      {k}: {n}");
                                }
                                Value::Bool(b) => {
                                    let _ = writeln!(yaml, "      {k}: {b}");
                                }
                                _ => {
                                    let _ = writeln!(yaml, "      {k}: {v}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(r) = &request.range {
        let _ = writeln!(yaml, "  range:");
        let _ = writeln!(yaml, "    start:");
        let _ = writeln!(yaml, "      line: {}", r.start.line);
        let _ = writeln!(yaml, "      column: {}", r.start.column);
        let _ = writeln!(yaml, "    end:");
        let _ = writeln!(yaml, "      line: {}", r.end.line);
        let _ = writeln!(yaml, "      column: {}", r.end.column);
    }

    // Relational rules.
    emit_value_yaml_field(&mut yaml, "  ", "has", request.has.as_deref());
    emit_value_yaml_field(&mut yaml, "  ", "inside", request.inside.as_deref());
    emit_value_yaml_field(&mut yaml, "  ", "follows", request.follows.as_deref());
    emit_value_yaml_field(&mut yaml, "  ", "precedes", request.precedes.as_deref());

    // Constraints.
    if let Some(constraints) = &request.constraints
        && !constraints.is_empty()
    {
        yaml.push_str("  constraints:\n");
        for (var_name, constraint_value) in constraints {
            yaml.push_str(&format!("    {var_name}:\n"));
            value_to_yaml(&mut yaml, constraint_value, 6);
        }
    }

    // Composite rules.
    if let Some(matches_name) = &request.matches {
        let _ = writeln!(yaml, "  matches: {}", yaml_escape_scalar(matches_name));
    }
    if let Some(all_rules) = &request.all
        && !all_rules.is_empty()
    {
        yaml.push_str("  all:\n");
        for rule in all_rules {
            yaml.push_str("    - ");
            match rule {
                Value::String(s) => {
                    let _ = writeln!(yaml, "pattern: {}", yaml_escape_scalar(s));
                }
                _ => {
                    yaml.push('\n');
                    value_to_yaml(&mut yaml, rule, 6);
                }
            }
        }
    }
    if let Some(any_rules) = &request.any
        && !any_rules.is_empty()
    {
        yaml.push_str("  any:\n");
        for rule in any_rules {
            yaml.push_str("    - ");
            match rule {
                Value::String(s) => {
                    let _ = writeln!(yaml, "pattern: {}", yaml_escape_scalar(s));
                }
                _ => {
                    yaml.push('\n');
                    value_to_yaml(&mut yaml, rule, 6);
                }
            }
        }
    }
    if let Some(not_rule) = &request.not {
        yaml.push_str("  not:\n");
        match not_rule.as_ref() {
            Value::String(s) => {
                let _ = writeln!(yaml, "    pattern: {}", yaml_escape_scalar(s));
            }
            _ => {
                value_to_yaml(&mut yaml, not_rule, 4);
            }
        }
    }

    // Emit transform pipeline if present.
    if let Some(transform) = &request.transform
        && !transform.is_empty()
    {
        yaml.push_str("transform:\n");
        for (var_name, transform_def) in transform {
            let _ = writeln!(yaml, "  {var_name}:");
            value_to_yaml(&mut yaml, transform_def, 4);
        }
    }

    yaml
}

/// Emit a relational rule field from a JSON value into YAML.
///
/// When the value is a bare string, it is emitted as `pattern: <value>` under
/// the field name (matching ast-grep's shorthand semantics where a string
/// relational rule means `{pattern: "..."}`).
pub(super) fn emit_value_yaml_field(
    yaml: &mut String,
    pad: &str,
    name: &str,
    value: Option<&Value>,
) {
    if let Some(val) = value {
        yaml.push_str(&format!("{pad}{name}:\n"));
        match val {
            Value::String(s) => {
                let child_pad = " ".repeat(pad.len() + 2);
                yaml.push_str(&format!("{child_pad}pattern: {}\n", yaml_escape_scalar(s)));
            }
            _ => {
                value_to_yaml(yaml, val, pad.len() + 2);
            }
        }
    }
}

/// Recursively serialize a JSON value to YAML at the given indentation.
pub(super) fn value_to_yaml(yaml: &mut String, value: &Value, indent: usize) {
    let pad = " ".repeat(indent);
    match value {
        Value::String(s) => {
            yaml.push_str(&format!("{pad}{}\n", yaml_escape_scalar(s)));
        }
        Value::Number(n) => {
            yaml.push_str(&format!("{pad}{n}\n"));
        }
        Value::Bool(b) => {
            yaml.push_str(&format!("{pad}{b}\n"));
        }
        Value::Null => {
            yaml.push_str(&format!("{pad}null\n"));
        }
        Value::Array(arr) => {
            for item in arr {
                yaml.push_str(&format!("{pad}- "));
                match item {
                    Value::String(s) => yaml.push_str(&format!("{}\n", yaml_escape_scalar(s))),
                    Value::Number(n) => yaml.push_str(&format!("{n}\n")),
                    Value::Bool(b) => yaml.push_str(&format!("{b}\n")),
                    _ => {
                        yaml.push('\n');
                        value_to_yaml(yaml, item, indent + 2);
                    }
                }
            }
        }
        Value::Object(obj) => {
            for (key, val) in obj {
                match val {
                    Value::Object(_) | Value::Array(_) => {
                        yaml.push_str(&format!("{pad}{key}:\n"));
                        value_to_yaml(yaml, val, indent + 2);
                    }
                    Value::String(s) => {
                        yaml.push_str(&format!("{pad}{key}: {}\n", yaml_escape_scalar(s)));
                    }
                    Value::Number(n) => {
                        yaml.push_str(&format!("{pad}{key}: {n}\n"));
                    }
                    Value::Bool(b) => {
                        yaml.push_str(&format!("{pad}{key}: {b}\n"));
                    }
                    Value::Null => {
                        yaml.push_str(&format!("{pad}{key}: null\n"));
                    }
                }
            }
        }
    }
}

/// Execute count via YAML rule generation (for nthChild/range/has/inside/constraints).
pub(super) fn build_fixconfig_rule_yaml(
    pattern: &str,
    lang: &str,
    fix_config: &FixConfig,
    selector: Option<&str>,
    transform: Option<&Map<String, Value>>,
) -> String {
    let mut yaml = String::new();
    yaml.push_str("id: fixconfig-rewrite\n");
    yaml.push_str(&format!("language: {lang}\n"));
    yaml.push_str("severity: info\n");
    yaml.push_str("rule:\n");

    if let Some(selector) = selector.filter(|s| !s.trim().is_empty()) {
        yaml.push_str(&format!("  pattern: {}\n", yaml_escape_scalar(pattern)));
        yaml.push_str(&format!("  selector: {}\n", yaml_escape_scalar(selector)));
    } else {
        yaml.push_str(&format!("  pattern: {}\n", yaml_escape_scalar(pattern)));
    }

    // Emit transform pipeline before fix so that transformed variables
    // can be referenced in the fix template.
    if let Some(transform) = transform
        && !transform.is_empty()
    {
        yaml.push_str("transform:\n");
        for (var_name, transform_def) in transform {
            use std::fmt::Write as _;
            let _ = writeln!(yaml, "  {var_name}:");
            value_to_yaml(&mut yaml, transform_def, 4);
        }
    }

    yaml.push_str("fix:\n");
    yaml.push_str(&format!(
        "  template: {}\n",
        yaml_escape_scalar(&fix_config.template)
    ));

    if let Some(expand_start) = &fix_config.expand_start {
        yaml.push_str("  expandStart:\n");
        append_expand_rule_yaml(&mut yaml, expand_start);
    }

    if let Some(expand_end) = &fix_config.expand_end {
        yaml.push_str("  expandEnd:\n");
        append_expand_rule_yaml(&mut yaml, expand_end);
    }

    yaml
}

/// Append expand rule fields to the YAML string, indented at the correct level.
pub(super) fn append_expand_rule_yaml(yaml: &mut String, rule: &FixExpandRule) {
    if let Some(regex) = &rule.regex {
        yaml.push_str(&format!("    regex: {}\n", yaml_escape_scalar(regex)));
    }
    if let Some(kind) = &rule.kind {
        yaml.push_str(&format!("    kind: {}\n", yaml_escape_scalar(kind)));
    }
    if let Some(pattern) = &rule.pattern {
        yaml.push_str(&format!("    pattern: {}\n", yaml_escape_scalar(pattern)));
    }
    if let Some(stop_by) = &rule.stop_by {
        match stop_by {
            Value::String(s) => {
                yaml.push_str(&format!("    stopBy: {}\n", yaml_escape_scalar(s)));
            }
            Value::Object(_) => {
                // For object stopBy, render as inline JSON-ish YAML.
                // This handles cases like `stopBy: { kind: "," }` or
                // `stopBy: { regex: "," }`.
                yaml.push_str("    stopBy:\n");
                if let Some(obj) = stop_by.as_object() {
                    for (key, val) in obj {
                        match val {
                            Value::String(s) => {
                                yaml.push_str(&format!(
                                    "      {}: {}\n",
                                    key,
                                    yaml_escape_scalar(s)
                                ));
                            }
                            Value::Number(n) => {
                                yaml.push_str(&format!("      {key}: {n}\n"));
                            }
                            Value::Bool(b) => {
                                yaml.push_str(&format!("      {key}: {b}\n"));
                            }
                            _ => {
                                yaml.push_str(&format!("      {key}: {val}\n"));
                            }
                        }
                    }
                }
            }
            _ => {
                yaml.push_str(&format!("    stopBy: {stop_by}\n"));
            }
        }
    }
}

/// Escape a string value for YAML output. Wraps in single quotes if the
/// value contains special YAML characters, and escapes internal single
/// quotes by doubling them.
pub(super) fn yaml_escape_scalar(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }

    let needs_quoting = value.contains(':')
        || value.contains('#')
        || value.contains('{')
        || value.contains('}')
        || value.contains('[')
        || value.contains(']')
        || value.contains(',')
        || value.contains('&')
        || value.contains('*')
        || value.contains('?')
        || value.contains('|')
        || value.contains('-')
        || value.contains('>')
        || value.contains('!')
        || value.contains('%')
        || value.contains('@')
        || value.contains('`')
        || value.contains('"')
        || value.contains('\'')
        || value.starts_with(' ')
        || value.ends_with(' ')
        || value == "true"
        || value == "false"
        || value == "null"
        || value == "yes"
        || value == "no"
        || value.parse::<f64>().is_ok();

    if needs_quoting {
        let escaped = value.replace('\'', "''");
        format!("'{escaped}'")
    } else {
        value.to_string()
    }
}
