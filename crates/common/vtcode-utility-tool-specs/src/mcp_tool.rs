use rmcp::model::Tool;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedMcpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Option<Value>,
}

#[must_use]
pub fn parse_mcp_tool(tool: &Tool) -> ParsedMcpTool {
    ParsedMcpTool {
        name: tool.name.to_string(),
        description: tool.description.clone().unwrap_or_default().to_string(),
        input_schema: serde_json::to_value(&tool.input_schema).unwrap_or(Value::Null),
        output_schema: tool.output_schema.as_ref().and_then(|schema| serde_json::to_value(schema).ok()),
    }
}

#[cfg(test)]
mod tests {
    use super::{ParsedMcpTool, parse_mcp_tool};
    use rmcp::model::Tool;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn parse_mcp_tool_preserves_name_description_and_input_schema() {
        let input_schema = Arc::new(
            serde_json::from_value(json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                }
            }))
            .expect("json object"),
        );
        let tool = Tool::new("search-docs", "Search documentation", input_schema);

        let parsed = parse_mcp_tool(&tool);
        assert_eq!(
            parsed,
            ParsedMcpTool {
                name: "search-docs".to_string(),
                description: "Search documentation".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"}
                    }
                }),
                output_schema: None,
            }
        );
    }

    #[test]
    fn parse_mcp_tool_defaults_missing_description() {
        let input_schema = Arc::new(serde_json::from_value(json!({})).expect("json object"));
        let tool = Tool::new_with_raw("search-docs", None, input_schema);

        let parsed = parse_mcp_tool(&tool);
        assert_eq!(parsed.description, "");
        assert_eq!(parsed.input_schema, json!({}));
        assert_eq!(parsed.output_schema, None);
    }

    #[test]
    fn parse_mcp_tool_captures_output_schema() {
        let input_schema = Arc::new(serde_json::from_value(json!({})).expect("json object"));
        let output_schema = Arc::new(
            serde_json::from_value(json!({
                "type": "object",
                "properties": {
                    "answer": {"type": "string"}
                },
                "required": ["answer"]
            }))
            .expect("json object"),
        );
        let mut tool = Tool::new_with_raw("ask_question", None, input_schema);
        tool.output_schema = Some(output_schema);

        let parsed = parse_mcp_tool(&tool);
        assert_eq!(
            parsed.output_schema,
            Some(json!({
                "type": "object",
                "properties": {
                    "answer": {"type": "string"}
                },
                "required": ["answer"]
            }))
        );
    }
}
