use hashbrown::HashMap;

use anstyle::{AnsiColor, Color, Style as AnsiStyle};
use vtcode_commons::diff_paths::{is_diff_addition_line, is_diff_deletion_line, is_diff_header_line};
use vtcode_core::config::constants::tools;
use vtcode_core::tools::tool_intent;
use vtcode_core::utils::diff_styles::{
    DiffColorLevel, DiffTheme, diff_add_bg, diff_add_word_bg, diff_del_bg, diff_del_word_bg, diff_hunk_bg,
};
use vtcode_core::utils::style_helpers::bold_color;

/// Get background color for diff lines based on detected theme and color level.
fn diff_line_bg_color(is_addition: bool) -> Option<Color> {
    let theme = DiffTheme::detect();
    let level = DiffColorLevel::detect();
    let bg = if is_addition {
        diff_add_bg(theme, level)
    } else {
        diff_del_bg(theme, level)
    };
    Some(bg)
}

fn diff_hunk_bg_color() -> Option<Color> {
    let theme = DiffTheme::detect();
    let level = DiffColorLevel::detect();
    if level == DiffColorLevel::Ansi16 {
        return None;
    }
    Some(diff_hunk_bg(theme, level))
}

pub(crate) struct GitStyles {
    pub(crate) add: Option<AnsiStyle>,
    pub(crate) remove: Option<AnsiStyle>,
    pub(crate) header: Option<AnsiStyle>,
    pub(crate) file_old: Option<AnsiStyle>,
    pub(crate) file_new: Option<AnsiStyle>,
    pub(crate) hunk: Option<AnsiStyle>,
    /// Stronger chips for word-level (intra-line) highlights on add rows.
    pub(crate) add_word: Option<AnsiStyle>,
    /// Stronger chips for word-level (intra-line) highlights on del rows.
    pub(crate) remove_word: Option<AnsiStyle>,
}

impl GitStyles {
    pub(crate) fn new() -> Self {
        // IntelliJ-style body rows: full-width add/del tint, default content
        // fg (syntax tokens keep their own colours), bright `+`/`-` only on
        // the gutter marker. Content never uses green-on-green / red-on-red.
        // Ansi16 cannot paint the tint, so content falls back to bright fg.
        // File/hunk headers are bold bands: `---` red, `+++` green, `@@` cyan
        // on a neutral tint — all full-width via line backgrounds.
        let ansi16 = DiffColorLevel::detect() == DiffColorLevel::Ansi16;
        let body_style = |is_addition: bool| {
            if ansi16 {
                let fg = if is_addition {
                    Color::Ansi(AnsiColor::BrightGreen)
                } else {
                    Color::Ansi(AnsiColor::BrightRed)
                };
                AnsiStyle::new().fg_color(Some(fg))
            } else {
                AnsiStyle::new().bg_color(diff_line_bg_color(is_addition))
            }
        };
        Self {
            add: Some(body_style(true)),
            remove: Some(body_style(false)),
            header: Some(
                AnsiStyle::new()
                    .fg_color(Some(Color::Ansi(AnsiColor::Cyan)))
                    .effects(anstyle::Effects::BOLD),
            ),
            file_old: Some(
                AnsiStyle::new()
                    .fg_color(Some(Color::Ansi(AnsiColor::BrightRed)))
                    .bg_color(diff_line_bg_color(false))
                    .effects(anstyle::Effects::BOLD),
            ),
            file_new: Some(
                AnsiStyle::new()
                    .fg_color(Some(Color::Ansi(AnsiColor::BrightGreen)))
                    .bg_color(diff_line_bg_color(true))
                    .effects(anstyle::Effects::BOLD),
            ),
            hunk: Some(
                AnsiStyle::new()
                    .fg_color(Some(Color::Ansi(AnsiColor::Cyan)))
                    .bg_color(diff_hunk_bg_color())
                    .effects(anstyle::Effects::BOLD),
            ),
            add_word: diff_word_bg_style(true),
            remove_word: diff_word_bg_style(false),
        }
    }
}

fn diff_word_bg_style(is_addition: bool) -> Option<AnsiStyle> {
    if DiffColorLevel::detect() == DiffColorLevel::Ansi16 {
        return None;
    }
    let theme = DiffTheme::detect();
    let level = DiffColorLevel::detect();
    let bg = if is_addition {
        diff_add_word_bg(theme, level)
    } else {
        diff_del_word_bg(theme, level)
    };
    Some(AnsiStyle::new().bg_color(Some(bg)))
}

pub(crate) struct LsStyles {
    classes: HashMap<String, AnsiStyle>,
    suffixes: Vec<(String, AnsiStyle)>,
}

impl LsStyles {
    pub(crate) fn from_env() -> Self {
        let mut classes: HashMap<String, AnsiStyle> = HashMap::new();
        let suffixes: Vec<(String, AnsiStyle)> = Vec::new();

        classes.insert("di".to_string(), bold_color(AnsiColor::Blue));
        classes.insert("ln".to_string(), bold_color(AnsiColor::Cyan));
        classes.insert("ex".to_string(), bold_color(AnsiColor::Green));
        classes.insert("pi".to_string(), bold_color(AnsiColor::Yellow));
        classes.insert("so".to_string(), bold_color(AnsiColor::Magenta));
        classes.insert("bd".to_string(), bold_color(AnsiColor::Yellow));
        classes.insert("cd".to_string(), bold_color(AnsiColor::Yellow));

        LsStyles { classes, suffixes }
    }

    pub(crate) fn style_for_line(&self, line: &str) -> Option<AnsiStyle> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let token = trimmed.split_whitespace().last().unwrap_or(trimmed).trim_matches('"');

        let mut name = token;
        let mut class_hint: Option<&str> = None;

        if let Some(stripped) = name.strip_suffix('/') {
            name = stripped;
            class_hint = Some("di");
        } else if let Some(stripped) = name.strip_suffix('@') {
            name = stripped;
            class_hint = Some("ln");
        } else if let Some(stripped) = name.strip_suffix('*') {
            name = stripped;
            class_hint = Some("ex");
        } else if let Some(stripped) = name.strip_suffix('|') {
            name = stripped;
            class_hint = Some("pi");
        } else if let Some(stripped) = name.strip_suffix('=') {
            name = stripped;
            class_hint = Some("so");
        }

        if class_hint.is_none() {
            match trimmed.chars().next() {
                Some('d') => class_hint = Some("di"),
                Some('l') => class_hint = Some("ln"),
                Some('p') => class_hint = Some("pi"),
                Some('s') => class_hint = Some("so"),
                Some('b') => class_hint = Some("bd"),
                Some('c') => class_hint = Some("cd"),
                _ => {}
            }
        }

        if let Some(code) = class_hint
            && let Some(style) = self.classes.get(code)
        {
            return Some(*style);
        }

        let lower = name
            .trim_matches(|c| matches!(c, '"' | ',' | ' ' | '\u{0009}'))
            .to_ascii_lowercase();
        for (suffix, style) in &self.suffixes {
            if lower.ends_with(suffix) {
                return Some(*style);
            }
        }

        if lower.ends_with('*')
            && let Some(style) = self.classes.get("ex")
        {
            return Some(*style);
        }

        None
    }

    #[cfg(test)]
    fn from_components(classes: HashMap<String, AnsiStyle>, suffixes: Vec<(String, AnsiStyle)>) -> Self {
        Self { classes, suffixes }
    }
}

pub(crate) fn select_line_style(
    tool_name: Option<&str>,
    line: &str,
    git: &GitStyles,
    ls: &LsStyles,
) -> Option<AnsiStyle> {
    let trimmed = line.trim_start();
    // Always detect and style diff lines, even when tool_name is not provided
    // (e.g. git_diff payloads routed through generic rendering path).
    // File headers get red/green bands, `@@` hunks get the neutral cyan band,
    // other metadata (`diff --git`, `index`, ...) stays dim with no band.
    if trimmed.starts_with("--- ") {
        return git.file_old;
    }
    if trimmed.starts_with("+++ ") {
        return git.file_new;
    }
    if trimmed.starts_with("@@") {
        return git.hunk.or(git.header);
    }
    if is_diff_header_line(trimmed) {
        return git.header;
    }
    if is_diff_addition_line(trimmed) {
        return git.add;
    }
    if is_diff_deletion_line(trimmed) {
        return git.remove;
    }

    if tool_name.is_some_and(|name| {
        tool_intent::canonical_command_session_tool_name(name).is_some()
            || matches!(name, tools::WRITE_FILE | tools::EDIT_FILE | tools::APPLY_PATCH)
    }) && let Some(style) = ls.style_for_line(trimmed)
    {
        return Some(style);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use anstyle::Effects;

    #[test]
    fn detects_git_diff_styling() {
        let git = GitStyles::new();
        let ls = LsStyles::from_components(HashMap::new(), Vec::new());
        let added = select_line_style(Some("run_pty_cmd"), "+added line", &git, &ls);
        assert_eq!(added, git.add);
        let removed = select_line_style(Some("run_pty_cmd"), "-removed line", &git, &ls);
        assert_eq!(removed, git.remove);
        let header = select_line_style(Some("run_pty_cmd"), "diff --git a/file b/file", &git, &ls);
        assert_eq!(header, git.header);
    }

    #[test]
    fn diff_content_styles_keep_default_fg_on_tint() {
        let git = GitStyles::new();
        let remove = git.remove.expect("remove style should exist");
        let add = git.add.expect("add style should exist");
        // DIMMED red on the maroon tint is unreadable; both sides stay solid.
        assert!(!remove.get_effects().contains(Effects::DIMMED));
        assert!(!add.get_effects().contains(Effects::DIMMED));
        // TrueColor/256: content keeps theme-default fg so code stays readable
        // on the tint; bright red/green lives only on the gutter marker.
        if DiffColorLevel::detect() == DiffColorLevel::Ansi16 {
            assert_eq!(remove.get_fg_color(), Some(Color::Ansi(AnsiColor::BrightRed)));
            assert_eq!(add.get_fg_color(), Some(Color::Ansi(AnsiColor::BrightGreen)));
            assert_eq!(remove.get_bg_color(), None);
            assert_eq!(add.get_bg_color(), None);
        } else {
            assert_eq!(remove.get_fg_color(), None);
            assert_eq!(add.get_fg_color(), None);
            assert!(remove.get_bg_color().is_some());
            assert!(add.get_bg_color().is_some());
        }
    }

    #[test]
    fn detects_ls_styles_for_directories_and_executables() {
        let git = GitStyles::new();
        use vtcode_core::utils::style_helpers::bold_color;
        let dir_style = bold_color(AnsiColor::Blue);
        let exec_style = bold_color(AnsiColor::Green);
        let mut classes = HashMap::new();
        classes.insert("di".to_string(), dir_style);
        classes.insert("ex".to_string(), exec_style);
        let ls = LsStyles::from_components(classes, Vec::new());
        let directory = select_line_style(Some("run_pty_cmd"), "folder/", &git, &ls);
        assert_eq!(directory, Some(dir_style));
        let executable = select_line_style(Some("run_pty_cmd"), "script*", &git, &ls);
        assert_eq!(executable, Some(exec_style));
    }

    #[test]
    fn non_terminal_tools_do_not_apply_special_styles() {
        let git = GitStyles::new();
        let ls = LsStyles::from_components(HashMap::new(), Vec::new());
        let styled = select_line_style(Some("context7"), "+added", &git, &ls);
        assert_eq!(styled, git.add);
    }

    #[test]
    fn diff_styling_works_without_tool_name() {
        let git = GitStyles::new();
        let ls = LsStyles::from_components(HashMap::new(), Vec::new());
        let header = select_line_style(None, "diff --git a/file b/file", &git, &ls);
        assert_eq!(header, git.header);
        let added = select_line_style(None, "+added", &git, &ls);
        assert_eq!(added, git.add);
    }

    #[test]
    fn file_and_hunk_headers_use_tinted_bold_bands() {
        let git = GitStyles::new();
        let ls = LsStyles::from_components(HashMap::new(), Vec::new());
        assert_eq!(select_line_style(None, "--- a/README.md", &git, &ls), git.file_old);
        assert_eq!(select_line_style(None, "+++ b/README.md", &git, &ls), git.file_new);
        assert_eq!(select_line_style(None, "@@ -100 +100 @@", &git, &ls), git.hunk);
        for style in [git.file_old, git.file_new] {
            let style = style.expect("header band style exists");
            assert!(style.get_bg_color().is_some());
            assert!(style.get_effects().contains(Effects::BOLD));
        }
        // Hunk tint is disabled on Ansi16 by design (no bg support), so only
        // assert the background when the terminal advertises more colors.
        let hunk = git.hunk.expect("header band style exists");
        assert!(hunk.get_effects().contains(Effects::BOLD));
        assert_eq!(hunk.get_bg_color().is_some(), DiffColorLevel::detect() != DiffColorLevel::Ansi16);
        let header = git.header.expect("metadata header exists");
        assert_eq!(header.get_bg_color(), None);
    }

    #[test]
    fn applies_extension_based_styles() {
        let git = GitStyles::new();
        use vtcode_core::utils::style_helpers::bold_color;
        let suffixes = vec![(".rs".to_string(), bold_color(AnsiColor::Red))];
        let ls = LsStyles::from_components(HashMap::new(), suffixes);
        let styled = select_line_style(Some("run_pty_cmd"), "main.rs", &git, &ls);
        assert!(styled.is_some());
    }

    #[test]
    fn extension_matching_requires_dot_boundary() {
        let git = GitStyles::new();
        use vtcode_core::utils::style_helpers::bold_color;
        let suffixes = vec![(".rs".to_string(), bold_color(AnsiColor::Green))];
        let ls = LsStyles::from_components(HashMap::new(), suffixes);

        let without_extension = select_line_style(Some("run_pty_cmd"), "helpers", &git, &ls);
        assert!(without_extension.is_none());

        let with_extension = select_line_style(Some("run_pty_cmd"), "helpers.rs", &git, &ls);
        assert!(with_extension.is_some());
    }
}
