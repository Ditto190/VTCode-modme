pub mod commands;
pub mod paths;
pub mod unified_path;

use serde_json::Value;

/// Extract a condensed representation of a JSON Schema for error hints.
///
/// Returns a JSON object with:
/// - `required`: array of required field names
/// - `properties`: object mapping field name -> its declared `type` (or `"any"` if absent)
///
/// This is intentionally compact so it can be included in validation error
/// payloads without bloating the context.
pub fn condensed_schema_hint(schema: &Value) -> Option<Value> {
    let properties = schema.get("properties").and_then(Value::as_object)?;
    let required: Vec<Value> = schema
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut prop_types = serde_json::Map::new();
    for (name, def) in properties {
        let type_str = def
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("any")
            .to_string();
        // Surface enum options inline (e.g. "string(grep|glob|list)") so a
        // model that passed an invalid value can self-correct instead of
        // retrying blind with the same malformed arguments.
        let rendered = match def.get("enum").and_then(Value::as_array) {
            Some(options) if !options.is_empty() => {
                let joined = options
                    .iter()
                    .map(|option| match option {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join("|");
                format!("{type_str}({joined})")
            }
            _ => type_str,
        };
        prop_types.insert(name.clone(), Value::String(rendered));
    }

    Some(serde_json::json!({
        "required": required,
        "properties": prop_types,
    }))
}
