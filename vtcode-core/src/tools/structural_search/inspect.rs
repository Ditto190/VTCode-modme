#![allow(unused_imports)]

#[allow(unused_imports)]
use super::*;

/// Best-effort extraction of `testConfigs` entries from a sgconfig.yml file.
/// Returns a list of objects with `testDir` (required) and `snapshotDir` (optional).
pub(super) async fn extract_test_configs(config_path: &Path) -> Vec<Value> {
    let Ok(content) = afs::read_to_string(config_path).await else {
        return Vec::new();
    };

    let mut configs = Vec::new();
    let mut in_test_configs = false;
    let mut current_item: Option<Map<String, Value>> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("testConfigs:") {
            in_test_configs = true;
            if trimmed.contains('[') {
                break; // Inline array of objects is too complex for line-by-line parsing
            }
            continue;
        }

        if !in_test_configs {
            continue;
        }

        // List item start: "- "
        if trimmed.starts_with("- ") {
            // Flush previous item
            if let Some(item) = current_item.take() {
                configs.push(Value::Object(item));
            }
            current_item = Some(Map::new());
            // Check for inline key-value: "- testDir: tests"
            let after_dash = trimmed.strip_prefix("- ").unwrap_or(trimmed).trim();
            if let Some((key, value)) = parse_yaml_simple_kv(after_dash)
                && let Some(ref mut item) = current_item
            {
                item.insert(key, value);
            }
            continue;
        }

        // A new top-level key (not a list item) ends the section
        if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
            if let Some(item) = current_item.take() {
                configs.push(Value::Object(item));
            }
            in_test_configs = false;
            continue;
        }

        // Key-value inside a list item (indented deeper than "- ")
        if let Some(ref mut item) = current_item
            && let Some((key, value)) = parse_yaml_simple_kv(trimmed)
        {
            item.insert(key, value);
        }
    }

    // Flush last item
    if let Some(item) = current_item {
        configs.push(Value::Object(item));
    }

    configs
}

/// Best-effort extraction of `utilDirs` entries from a sgconfig.yml file.
/// Returns relative directory paths found under the `utilDirs:` key.
pub(super) async fn extract_util_dirs(config_path: &Path) -> Vec<String> {
    extract_string_list_from_yaml(config_path, "utilDirs").await
}

/// Generic helper to extract a YAML string list from a sgconfig.yml key.
/// Handles both inline arrays (`key: [a, b]`) and block sequences (`key:\n  - a\n  - b`).
pub(super) async fn extract_string_list_from_yaml(config_path: &Path, key: &str) -> Vec<String> {
    let Ok(content) = afs::read_to_string(config_path).await else {
        return Vec::new();
    };

    let mut dirs = Vec::new();
    let mut in_section = false;
    let key_prefix = format!("{key}:");

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&key_prefix) {
            in_section = true;
            // Handle inline array: key: [a, b]
            if let Some(bracket_content) = trimmed.strip_prefix(&key_prefix).map(str::trim)
                && bracket_content.starts_with('[')
            {
                let inner = bracket_content.trim_matches(|c| c == '[' || c == ']');
                for item in inner.split(',') {
                    let item = item.trim().trim_matches('"').trim_matches('\'');
                    if !item.is_empty() {
                        dirs.push(item.to_string());
                    }
                }
                in_section = false;
            }
            continue;
        }

        if in_section {
            if trimmed.starts_with('-') {
                let item = trimmed
                    .strip_prefix('-')
                    .unwrap_or(trimmed)
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                if !item.is_empty() {
                    dirs.push(item.to_string());
                }
            } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
                // Hit a new key, stop collecting
                in_section = false;
            }
        }
    }

    dirs
}

/// Best-effort extraction of `languageInjections` entries from a sgconfig.yml file.
/// Returns a list of objects with `host_language`, `rule_pattern` (or `rule_kind`),
/// and `injected` language name.
pub(super) async fn extract_language_injections(config_path: &Path) -> Vec<Value> {
    let Ok(content) = afs::read_to_string(config_path).await else {
        return Vec::new();
    };

    let mut injections = Vec::new();
    let mut in_injections = false;
    let mut current_item: Option<Map<String, Value>> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        // Detect start of languageInjections section
        if trimmed.starts_with("languageInjections:") {
            in_injections = true;
            // Handle inline array (uncommon but possible)
            if trimmed.contains('[') {
                break; // Inline array of objects is too complex for line-by-line parsing
            }
            continue;
        }

        if !in_injections {
            continue;
        }

        // List item start: "- " (may be at 0-indent in YAML)
        if trimmed.starts_with("- ") {
            // Flush previous item
            if let Some(item) = current_item.take() {
                injections.push(Value::Object(item));
            }
            current_item = Some(Map::new());
            // Check for inline key-value: "- hostLanguage: js"
            let after_dash = trimmed.strip_prefix("- ").unwrap_or(trimmed).trim();
            if let Some((key, value)) = parse_yaml_simple_kv(after_dash)
                && let Some(ref mut item) = current_item
            {
                item.insert(key, value);
            }
            continue;
        }

        // A new top-level key (not a list item) ends the section
        if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
            if let Some(item) = current_item.take() {
                injections.push(Value::Object(item));
            }
            in_injections = false;
            continue;
        }

        // Key-value inside a list item (indented deeper than "- ")
        if let Some(ref mut item) = current_item
            && let Some((key, value)) = parse_yaml_simple_kv(trimmed)
        {
            item.insert(key, value);
        }
    }

    // Flush last item
    if let Some(item) = current_item {
        injections.push(Value::Object(item));
    }

    injections
}

/// Best-effort extraction of `customLanguages` entries from a sgconfig.yml file.
/// Returns a JSON object mapping language names to their config (library_path, extensions).
pub(super) async fn extract_custom_languages(config_path: &Path) -> Value {
    let Ok(content) = afs::read_to_string(config_path).await else {
        return Value::Object(Map::new());
    };

    let mut languages = Map::new();
    let mut in_custom_languages = false;
    let mut current_lang: Option<String> = None;
    let mut current_lang_config: Option<Map<String, Value>> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("customLanguages:") {
            in_custom_languages = true;
            continue;
        }

        if !in_custom_languages {
            continue;
        }

        // A new top-level key ends the section
        if !line.starts_with(' ')
            && !line.starts_with('\t')
            && !trimmed.is_empty()
            && !trimmed.starts_with('#')
        {
            if let (Some(lang), Some(config)) = (current_lang.take(), current_lang_config.take()) {
                languages.insert(lang, Value::Object(config));
            }
            in_custom_languages = false;
            continue;
        }

        // Language entry at 2-space indent: "graphql:"
        let indent = line.len() - line.trim_start().len();
        if indent == 2 && trimmed.ends_with(':') && !trimmed.contains(' ') {
            // Flush previous language
            if let (Some(lang), Some(config)) = (current_lang.take(), current_lang_config.take()) {
                languages.insert(lang, Value::Object(config));
            }
            current_lang = Some(trimmed.trim_end_matches(':').to_string());
            current_lang_config = Some(Map::new());
            continue;
        }

        // Key-value inside a language entry
        if let Some(ref mut config) = current_lang_config
            && let Some((key, value)) = parse_yaml_simple_kv(trimmed)
        {
            config.insert(key, value);
        }
    }

    // Flush last language
    if let (Some(lang), Some(config)) = (current_lang, current_lang_config) {
        languages.insert(lang, Value::Object(config));
    }

    Value::Object(languages)
}

/// Best-effort extraction of `languageGlobs` entries from a sgconfig.yml file.
/// Returns a JSON object mapping language names to their glob pattern arrays.
pub(super) async fn extract_language_globs(config_path: &Path) -> Value {
    let Ok(content) = afs::read_to_string(config_path).await else {
        return Value::Object(Map::new());
    };

    let mut globs = Map::new();
    let mut in_language_globs = false;
    let mut current_lang: Option<String> = None;
    let mut current_patterns: Vec<Value> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("languageGlobs:") {
            in_language_globs = true;
            continue;
        }

        if !in_language_globs {
            continue;
        }

        // A new top-level key ends the section
        if !line.starts_with(' ')
            && !line.starts_with('\t')
            && !trimmed.is_empty()
            && !trimmed.starts_with('#')
        {
            if let Some(lang) = current_lang.take() {
                globs.insert(lang, Value::Array(std::mem::take(&mut current_patterns)));
            }
            in_language_globs = false;
            continue;
        }

        let indent = line.len() - line.trim_start().len();

        // Language entry at 2-space indent: "tsx:"
        if indent == 2 && trimmed.ends_with(':') && !trimmed.starts_with('-') {
            // Flush previous language
            if let Some(lang) = current_lang.take() {
                globs.insert(lang, Value::Array(std::mem::take(&mut current_patterns)));
            }
            current_lang = Some(trimmed.trim_end_matches(':').to_string());
            continue;
        }

        // Glob pattern entry: "- \"*.tsx\"" at 4-space indent
        if indent >= 4 && trimmed.starts_with("- ") {
            let pattern = trimmed
                .strip_prefix("- ")
                .unwrap_or(trimmed)
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            if !pattern.is_empty() {
                current_patterns.push(Value::String(pattern.to_string()));
            }
        }
    }

    // Flush last language
    if let Some(lang) = current_lang {
        globs.insert(lang, Value::Array(current_patterns));
    }

    Value::Object(globs)
}
