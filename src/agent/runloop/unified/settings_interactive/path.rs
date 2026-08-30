use anyhow::{Context, Result, bail};
use toml::Value as TomlValue;

#[derive(Debug, Clone)]
pub(super) enum PathToken {
    Key(String),
    Index(usize),
}

pub(crate) fn parent_view_path(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }

    if path.ends_with(']')
        && let Some(start) = path.rfind('[')
    {
        let parent = &path[..start];
        return (!parent.is_empty()).then(|| parent.to_string());
    }

    path.rfind('.').map(|idx| path[..idx].to_string())
}

pub(super) fn parse_path_tokens(path: &str) -> Result<Vec<PathToken>> {
    let mut tokens = Vec::new();

    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }

        let mut rest = segment;
        loop {
            if let Some(index_start) = rest.find('[') {
                let key = &rest[..index_start];
                if !key.is_empty() {
                    tokens.push(PathToken::Key(key.to_string()));
                }

                let after_start = &rest[index_start + 1..];
                let Some(index_end) = after_start.find(']') else {
                    bail!("Invalid path segment '{segment}': missing closing bracket");
                };

                let index_text = &after_start[..index_end];
                let index = index_text
                    .parse::<usize>()
                    .with_context(|| format!("Invalid array index '{index_text}'"))?;
                tokens.push(PathToken::Index(index));

                rest = &after_start[index_end + 1..];
                if rest.is_empty() {
                    break;
                }
            } else {
                tokens.push(PathToken::Key(rest.to_string()));
                break;
            }
        }
    }

    Ok(tokens)
}

pub(super) fn get_node<'a>(root: &'a TomlValue, path: &str) -> Option<&'a TomlValue> {
    let tokens = parse_path_tokens(path).ok()?;
    let mut current = root;

    for token in tokens {
        match token {
            PathToken::Key(key) => {
                let TomlValue::Table(table) = current else {
                    return None;
                };
                current = table.get(&key)?;
            }
            PathToken::Index(index) => {
                let TomlValue::Array(entries) = current else {
                    return None;
                };
                current = entries.get(index)?;
            }
        }
    }

    Some(current)
}

pub(super) fn get_node_mut<'a>(root: &'a mut TomlValue, path: &str) -> Option<&'a mut TomlValue> {
    let tokens = parse_path_tokens(path).ok()?;
    let mut current = root;

    for token in tokens {
        match token {
            PathToken::Key(key) => {
                let TomlValue::Table(table) = current else {
                    return None;
                };
                current = table.get_mut(&key)?;
            }
            PathToken::Index(index) => {
                let TomlValue::Array(entries) = current else {
                    return None;
                };
                current = entries.get_mut(index)?;
            }
        }
    }

    Some(current)
}

pub(super) fn set_node(root: &mut TomlValue, path: &str, value: TomlValue) -> Result<()> {
    let tokens = parse_path_tokens(path)?;
    if tokens.is_empty() {
        bail!("Settings path '{path}' was not found");
    }

    set_node_tokens(root, &tokens, value, path)
}

fn set_node_tokens(current: &mut TomlValue, tokens: &[PathToken], value: TomlValue, path: &str) -> Result<()> {
    let Some(token) = tokens.first() else {
        *current = value;
        return Ok(());
    };

    match token {
        PathToken::Key(key) => {
            let TomlValue::Table(table) = current else {
                bail!("Settings path '{path}' traverses a non-table value");
            };

            if tokens.len() == 1 {
                table.insert(key.clone(), value);
                return Ok(());
            }

            let child = table.entry(key.clone()).or_insert_with(|| match tokens[1] {
                PathToken::Key(_) => TomlValue::Table(toml::map::Map::new()),
                PathToken::Index(_) => TomlValue::Array(Vec::new()),
            });
            set_node_tokens(child, &tokens[1..], value, path)
        }
        PathToken::Index(index) => {
            let TomlValue::Array(entries) = current else {
                bail!("Settings path '{path}' traverses a non-array value");
            };
            let entry = entries
                .get_mut(*index)
                .ok_or_else(|| anyhow::anyhow!("Settings path '{path}' references a missing array item"))?;
            set_node_tokens(entry, &tokens[1..], value, path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_node_updates_nested_array_value() {
        let mut root: TomlValue = toml::from_str(
            r#"
            [[hooks.lifecycle.pre_tool_use]]
            hooks = ["before"]
            "#,
        )
        .expect("valid TOML");

        set_node(&mut root, "hooks.lifecycle.pre_tool_use[0].hooks[0]", TomlValue::String("after".to_string()))
            .expect("nested array value should be updated");

        assert_eq!(
            get_node(&root, "hooks.lifecycle.pre_tool_use[0].hooks[0]").and_then(TomlValue::as_str),
            Some("after")
        );
    }

    #[test]
    fn set_node_creates_missing_tables() {
        let mut root = TomlValue::Table(toml::Table::new());

        set_node(&mut root, "agent.small_model.model", TomlValue::String("small-model".to_string()))
            .expect("missing tables should be created");

        assert_eq!(get_node(&root, "agent.small_model.model").and_then(TomlValue::as_str), Some("small-model"));
    }
}
