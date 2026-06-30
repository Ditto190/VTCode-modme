#![allow(unused_imports)]

#[allow(unused_imports)]
use super::*;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub(super) enum StructuralWorkflow {
    #[default]
    Query,
    Scan,
    Test,
    Inspect,
    Rewrite,
    Count,
    Rules,
    New,
    Apply,
}

impl StructuralWorkflow {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Scan => "scan",
            Self::Test => "test",
            Self::Inspect => "inspect",
            Self::Rewrite => "rewrite",
            Self::Count => "count",
            Self::Rules => "rules",
            Self::New => "new",
            Self::Apply => "apply",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StructuralStrictness {
    Cst,
    Smart,
    Ast,
    Relaxed,
    Signature,
    Template,
}

impl StructuralStrictness {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::Cst => "cst",
            Self::Smart => "smart",
            Self::Ast => "ast",
            Self::Relaxed => "relaxed",
            Self::Signature => "signature",
            Self::Template => "template",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DebugQueryFormat {
    Pattern,
    Ast,
    Cst,
    Sexp,
}

impl DebugQueryFormat {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::Pattern => "pattern",
            Self::Ast => "ast",
            Self::Cst => "cst",
            Self::Sexp => "sexp",
        }
    }
}

/// Accepted forms for the `nth_child` field: a plain number, an An+B
/// formula string, or a full object with `position`, optional `reverse`,
/// and optional `ofRule`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum NthChildInput {
    Number(usize),
    Formula(String),
    Object(NthChildObject),
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct NthChildObject {
    pub(super) position: Value,
    #[serde(default)]
    pub(super) reverse: Option<bool>,
    #[serde(default, rename = "ofRule")]
    pub(super) of_rule: Option<Value>,
}

/// A source-range constraint with 0-based line/column positions.
/// `start` is inclusive, `end` is exclusive.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct RangeInput {
    pub(super) start: RangePoint,
    pub(super) end: RangePoint,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct RangePoint {
    pub(super) line: usize,
    pub(super) column: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum GlobInput {
    Single(String),
    Multiple(Vec<String>),
}

impl GlobInput {
    pub(super) fn into_vec(self) -> Vec<String> {
        match self {
            Self::Single(glob) => vec![glob],
            Self::Multiple(globs) => globs,
        }
    }
}

/// A rule object used in `expandStart` / `expandEnd` of a `FixConfig`.
/// Supports the common rule forms: `regex`, `kind`, `pattern`, plus the
/// optional `stopBy` field unique to expand rules.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct FixExpandRule {
    #[serde(default)]
    pub(super) regex: Option<String>,
    #[serde(default)]
    pub(super) kind: Option<String>,
    #[serde(default)]
    pub(super) pattern: Option<String>,
    /// Controls where the expansion stops. Defaults to `"end"` (expand to
    /// the end of the enclosing node). Set to `"line"` to stop at end of
    /// line, or a rule object to stop at a specific sibling.
    #[serde(default)]
    pub(super) stop_by: Option<Value>,
}

impl FixExpandRule {
    pub(super) fn is_empty(&self) -> bool {
        self.regex.is_none() && self.kind.is_none() && self.pattern.is_none()
    }

    pub(super) fn validate(&self, label: &str) -> Result<()> {
        if self.is_empty() {
            bail!("`{label}` must specify at least one of `regex`, `kind`, or `pattern`");
        }
        Ok(())
    }

    /// Serialize this expand rule to a YAML-compatible JSON value for rule
    /// file generation.
    pub(super) fn to_yaml_value(&self) -> Value {
        let mut obj = Map::new();
        if let Some(regex) = &self.regex {
            obj.insert("regex".to_string(), Value::String(regex.clone()));
        }
        if let Some(kind) = &self.kind {
            obj.insert("kind".to_string(), Value::String(kind.clone()));
        }
        if let Some(pattern) = &self.pattern {
            obj.insert("pattern".to_string(), Value::String(pattern.clone()));
        }
        if let Some(stop_by) = &self.stop_by {
            obj.insert("stopBy".to_string(), stop_by.clone());
        }
        Value::Object(obj)
    }
}

/// Advanced fix configuration that allows expanding the replacement range
/// beyond the matched AST node. This maps to ast-grep's `FixConfig` YAML
/// rule feature.
///
/// Use `FixConfig` when replacing only the matched node is not enough,
/// especially for deleting list items or key-value pairs that also need
/// a surrounding comma removed.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct FixConfig {
    /// The replacement template string. Meta variables from the matched
    /// pattern can be referenced here (e.g. `$VAR`, `$$$ARGS`).
    pub(super) template: String,
    /// Optional rule to expand the fix range start backwards. The range
    /// start moves left until the rule is no longer met.
    #[serde(default)]
    pub(super) expand_start: Option<FixExpandRule>,
    /// Optional rule to expand the fix range end forwards. The range end
    /// moves right until the rule is no longer met.
    #[serde(default)]
    pub(super) expand_end: Option<FixExpandRule>,
}

impl FixConfig {
    pub(super) fn validate(&self) -> Result<()> {
        // Template can be empty for "delete" operations (replace matched
        // node with nothing). Validation ensures the field is present.
        if let Some(expand_start) = &self.expand_start {
            expand_start.validate("fix_config.expand_start")?;
        }
        if let Some(expand_end) = &self.expand_end {
            expand_end.validate("fix_config.expand_end")?;
        }
        Ok(())
    }

    pub(super) fn has_expansion(&self) -> bool {
        self.expand_start.is_some() || self.expand_end.is_some()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct StructuralSearchRequest {
    #[serde(default)]
    pub(super) workflow: StructuralWorkflow,
    #[serde(default)]
    pub(super) pattern: Option<String>,
    #[serde(default)]
    pub(super) kind: Option<String>,
    #[serde(default)]
    pub(super) path: Option<String>,
    #[serde(default)]
    pub(super) config_path: Option<String>,
    #[serde(default)]
    pub(super) filter: Option<String>,
    #[serde(default)]
    pub(super) lang: Option<String>,
    #[serde(default)]
    pub(super) selector: Option<String>,
    #[serde(default)]
    pub(super) strictness: Option<StructuralStrictness>,
    #[serde(default)]
    pub(super) debug_query: Option<DebugQueryFormat>,
    #[serde(default)]
    pub(super) globs: Option<GlobInput>,
    /// Glob patterns to exclude from the search. Each entry is a glob
    /// pattern (e.g. `"*.md"`, `"tests/**"`). These are converted to
    /// negative `--globs` flags (`!pattern`) for ast-grep. Can be a
    /// single string or an array of strings.
    #[serde(default, alias = "exclude")]
    pub(super) exclude: Option<GlobInput>,
    #[serde(default)]
    pub(super) context_lines: Option<usize>,
    #[serde(default)]
    pub(super) max_results: Option<usize>,
    #[serde(default)]
    pub(super) skip_snapshot_tests: Option<bool>,
    /// Update all snapshot files without interactive confirmation.
    /// Only valid for `test` workflow. Passed as `--update-all` to `sg test`.
    #[serde(default)]
    pub(super) update_all: Option<bool>,
    /// Launch an interactive session to accept/reject snapshot updates.
    /// Only valid for `test` workflow. Passed as `--interactive` to `sg test`.
    #[serde(default)]
    pub(super) interactive: Option<bool>,
    /// Override the test directory for sg test.
    /// Only valid for `test` workflow. Passed as `--test-dir` to `sg test`.
    #[serde(default)]
    pub(super) test_dir: Option<String>,
    /// Override the snapshot directory for sg test.
    /// Only valid for `test` workflow. Passed as `--snapshot-dir` to `sg test`.
    #[serde(default)]
    pub(super) snapshot_dir: Option<String>,
    /// Include `severity: off` rules in test.
    /// Only valid for `test` workflow. Passed as `--include-off` to `sg test`.
    #[serde(default)]
    pub(super) include_off: Option<bool>,
    #[serde(default)]
    pub(super) rewrite: Option<String>,
    /// Advanced fix configuration for the rewrite workflow. When present,
    /// the tool generates a temporary YAML rule with `fix` as a `FixConfig`
    /// object (template + expandStart/expandEnd) and runs `sg scan` instead
    /// of `sg run --rewrite`.
    #[serde(default, rename = "fix_config")]
    pub(super) fix_config: Option<FixConfig>,

    /// Match node text by Rust regex. Passed as `--regex` to the ast-grep
    /// CLI. Requires `lang` to be set. Only valid for `query` and `rewrite`
    /// workflows.
    #[serde(default)]
    pub(super) regex: Option<String>,

    /// Match by 1-based position among named siblings. Accepts a number,
    /// an An+B formula string, or an object with `position`, optional
    /// `reverse`, and optional `ofRule`. Only valid for `query` workflow;
    /// triggers YAML rule generation.
    #[serde(default, rename = "nth_child")]
    pub(super) nth_child: Option<NthChildInput>,

    /// Match by source position (0-based line/column, start inclusive, end
    /// exclusive). Only valid for `query` workflow; triggers YAML rule
    /// generation.
    #[serde(default)]
    pub(super) range: Option<RangeInput>,

    // -- Relational rule fields ------------------------------------------------
    /// Relational: match if a descendant matches this rule.
    #[serde(default)]
    pub(super) has: Option<Box<Value>>,
    /// Relational: match if an ancestor matches this rule.
    #[serde(default)]
    pub(super) inside: Option<Box<Value>>,
    /// Relational: match if a preceding sibling matches this rule.
    #[serde(default)]
    pub(super) follows: Option<Box<Value>>,
    /// Relational: match if a following sibling matches this rule.
    #[serde(default)]
    pub(super) precedes: Option<Box<Value>>,
    /// Narrow meta-variable matches by additional constraints.
    #[serde(default)]
    pub(super) constraints: Option<Map<String, Value>>,

    // -- Composite rule fields -------------------------------------------------
    /// Composite: reference a utility rule by name via `matches`.
    #[serde(default)]
    pub(super) matches: Option<String>,
    /// Composite: all sub-rules must match (conjunction).
    #[serde(default)]
    pub(super) all: Option<Vec<Value>>,
    /// Composite: any sub-rule must match (disjunction).
    #[serde(default)]
    pub(super) any: Option<Vec<Value>>,
    /// Composite: the sub-rule must not match (negation).
    #[serde(default)]
    pub(super) not: Option<Box<Value>>,
    /// Local utility rules defined inline for this query. Each key is a
    /// utility rule id and each value is the rule object.
    #[serde(default)]
    pub(super) utils: Option<Map<String, Value>>,

    // -- Transform fields ------------------------------------------------------
    /// Transform pipeline for meta-variable substitution. Each key is a
    /// new variable name and each value defines the transform operation
    /// (replace, substring, or convert). Transformed variables can be
    /// referenced in `fix_config.template` via `$$$VAR_NAME`.
    ///
    /// Only valid for `query`, `count`, and `rewrite` workflows that use
    /// YAML rule generation. Requires `lang` to be set.
    #[serde(default)]
    pub(super) transform: Option<Map<String, Value>>,

    // -- Scan-specific fields ---------------------------------------------------
    /// Post-run severity filter for `scan` workflow. When present, only
    /// findings whose severity matches one of the listed values are returned.
    /// Valid values: `error`, `warning`, `info`, `hint`. This filters the
    /// output after ast-grep runs; it does not override rule severities.
    #[serde(default)]
    pub(super) severities: Option<Vec<String>>,

    /// Control which ignore files ast-grep respects. Valid values:
    /// `hidden`, `dot`, `exclude`, `global`, `parent`, `vcs`.
    /// Only valid for `scan`, `query`, and `rewrite` workflows.
    #[serde(default, alias = "no-ignore")]
    pub(super) no_ignore: Option<Vec<String>>,

    /// Follow symbolic links while traversing directories.
    /// Only valid for `scan`, `query`, and `rewrite` workflows.
    #[serde(default)]
    pub(super) follow: Option<bool>,

    /// Number of threads for ast-grep to use. 0 means auto.
    /// Only valid for `scan` workflow. Max 256.
    #[serde(default)]
    pub(super) threads: Option<u32>,

    /// Output format for structural `workflow="scan"`.
    /// Valid values: `github`, `sarif`, `files_with_matches`, `count`.
    /// `github`/`sarif`: CI pipeline formats, raw output returned.
    /// `files_with_matches`: return only unique file paths with matches.
    /// `count`: return match counts per file.
    #[serde(default)]
    pub(super) format: Option<String>,

    /// Diagnostic report style. Valid values: `rich`, `medium`, `short`.
    /// Only valid for `scan` workflow.
    #[serde(default, alias = "report-style")]
    pub(super) report_style: Option<String>,

    /// Number of context lines to show before each match. Mutually
    /// exclusive with `context_lines`. Only valid for `query`, `scan`,
    /// and `rewrite` workflows.
    #[serde(default, alias = "before-lines")]
    pub(super) before_lines: Option<usize>,

    /// Number of context lines to show after each match. Mutually
    /// exclusive with `context_lines`. Only valid for `query`, `scan`,
    /// and `rewrite` workflows.
    #[serde(default, alias = "after-lines")]
    pub(super) after_lines: Option<usize>,

    /// Built-in ast-grep rules to activate. Valid values:
    /// `unused-suppression`, `no-suppress-all`. Each entry is activated
    /// at the severity specified in the format `"rule-id:severity"`
    /// (e.g. `"unused-suppression:error"`). If no severity is specified,
    /// defaults to the rule's built-in severity.
    /// Only valid for `scan` workflow.
    #[serde(default, alias = "builtin-rules")]
    pub(super) builtin_rules: Option<Vec<String>>,

    // -- New workflow fields ---------------------------------------------------
    /// Subcommand for `workflow='new'`: `project`, `rule`, `test`, or `util`.
    #[serde(default, rename = "new_subcommand")]
    pub(super) new_subcommand: Option<String>,

    /// Name of the rule, test, or utility to create.
    /// Required for `new` subcommands `rule`, `test`, and `util`.
    #[serde(default, rename = "new_name")]
    pub(super) new_name: Option<String>,
}

impl StructuralSearchRequest {
    pub(super) fn from_args(args: &Value) -> Result<Self> {
        reject_forbidden_args(args)?;

        let mut request: Self = deserialize_tool_args(args, "structural_search")?;
        request.normalize();
        request.validate()?;

        Ok(request)
    }

    pub(super) fn normalize(&mut self) {
        if self.workflow == StructuralWorkflow::Query
            || self.workflow == StructuralWorkflow::Rewrite
            || self.workflow == StructuralWorkflow::Count
            || self.workflow == StructuralWorkflow::Apply
        {
            self.lang = self.normalized_or_inferred_lang();
        }
    }

    pub(super) fn validate(&self) -> Result<()> {
        self.validate_limits()?;

        match self.workflow {
            StructuralWorkflow::Query => self.validate_query(),
            StructuralWorkflow::Scan => self.validate_scan(),
            StructuralWorkflow::Test => self.validate_test(),
            StructuralWorkflow::Inspect => self.validate_inspect(),
            StructuralWorkflow::Rewrite => self.validate_rewrite(),
            StructuralWorkflow::Count => self.validate_query(),
            StructuralWorkflow::Rules => self.validate_scan(),
            StructuralWorkflow::New => self.validate_new(),
            StructuralWorkflow::Apply => self.validate_apply(),
        }
    }

    pub(super) fn validate_limits(&self) -> Result<()> {
        let glob_count = self.normalized_globs().len();
        if glob_count > MAX_ALLOWED_GLOBS {
            bail!(
                "action='structural' accepts at most {MAX_ALLOWED_GLOBS} non-empty `globs` entries"
            );
        }

        if let Some(context_lines) = self.context_lines
            && context_lines > MAX_ALLOWED_CONTEXT_LINES
        {
            bail!(
                "action='structural' accepts at most {MAX_ALLOWED_CONTEXT_LINES} `context_lines`"
            );
        }

        // Validate no_ignore values.
        if let Some(no_ignore) = &self.no_ignore {
            for value in no_ignore {
                let normalized = value.trim().to_ascii_lowercase();
                if !VALID_NO_IGNORE_VALUES.contains(&normalized.as_str()) {
                    bail!(
                        "invalid `no_ignore` value `{value}`; expected one of: {}",
                        VALID_NO_IGNORE_VALUES.join(", ")
                    );
                }
            }
        }

        // Validate mutual exclusivity of context_lines vs before_lines/after_lines.
        if self.context_lines.is_some()
            && (self.before_lines.is_some() || self.after_lines.is_some())
        {
            bail!(
                "`context_lines` is mutually exclusive with `before_lines` and `after_lines`; use one or the other"
            );
        }

        Ok(())
    }

    pub(super) fn validate_query(&self) -> Result<()> {
        let has_relational = self.has.is_some()
            || self.inside.is_some()
            || self.follows.is_some()
            || self.precedes.is_some();

        let has_composite = self.matches.is_some() || self.all.is_some() || self.any.is_some();

        if self.pattern().is_none()
            && self.kind().is_none()
            && self.regex_pattern().is_none()
            && self.nth_child.is_none()
            && self.range.is_none()
            && !has_relational
            && !has_composite
            && self.constraints.is_none()
        {
            bail!(
                "action='structural' workflow='query' requires a non-empty `pattern`, `kind`, \
                 `regex`, `nth_child`, `range`, `has`, `inside`, `follows`, `precedes`, \
                 `matches`, `all`, or `any`"
            );
        }

        self.reject_present("config_path", self.config_path.as_deref())?;
        self.reject_present("filter", self.filter.as_deref())?;
        self.reject_flag("skip_snapshot_tests", self.skip_snapshot_tests)?;
        self.reject_flag("update_all", self.update_all)?;
        self.reject_flag("interactive", self.interactive)?;
        self.reject_present("test_dir", self.test_dir.as_deref())?;
        self.reject_present("snapshot_dir", self.snapshot_dir.as_deref())?;
        self.reject_flag("include_off", self.include_off)?;

        if self.debug_query.is_some() && self.lang.as_deref().is_none_or(str::is_empty) {
            bail!(DEBUG_QUERY_LANG_HINT);
        }

        if self.regex_pattern().is_some() && self.lang.as_deref().is_none_or(str::is_empty) {
            bail!("action='structural' with `regex` requires `lang` to be set");
        }

        if has_relational && self.lang.as_deref().is_none_or(str::is_empty) {
            bail!(
                "action='structural' with relational rules (`has`/`inside`/`follows`/`precedes`) requires `lang` to be set"
            );
        }

        if has_composite && self.lang.as_deref().is_none_or(str::is_empty) {
            bail!(
                "action='structural' with composite rules (`matches`/`all`/`any`) requires `lang` to be set"
            );
        }

        if self.transform.is_some() && self.lang.as_deref().is_none_or(str::is_empty) {
            bail!(
                "action='structural' with `transform` requires `lang` to be set because transform \
                 definitions are emitted into YAML rules that target a specific language"
            );
        }

        self.validate_nth_child_position()?;

        Ok(())
    }

    pub(super) fn validate_nth_child_position(&self) -> Result<()> {
        if let Some(ref nth) = self.nth_child {
            match nth {
                NthChildInput::Number(n) => {
                    if *n == 0 {
                        bail!("`nth_child` position is 1-based; 0 is not valid");
                    }
                }
                NthChildInput::Object(obj) => {
                    if let Some(pos) = obj.position.as_u64()
                        && pos == 0
                    {
                        bail!("`nth_child` position is 1-based; 0 is not valid");
                    }
                }
                NthChildInput::Formula(_) => {
                    // An+B formulas are validated by ast-grep itself.
                }
            }
        }
        Ok(())
    }

    pub(super) fn validate_scan(&self) -> Result<()> {
        self.reject_present("pattern", self.pattern.as_deref())?;
        self.reject_present("kind", self.kind.as_deref())?;
        self.reject_present("lang", self.lang.as_deref())?;
        self.reject_present("selector", self.selector.as_deref())?;
        self.reject_present(
            "strictness",
            self.strictness.as_ref().map(StructuralStrictness::as_str),
        )?;
        self.reject_present(
            "debug_query",
            self.debug_query.as_ref().map(DebugQueryFormat::as_str),
        )?;
        self.reject_present("regex", self.regex.as_deref())?;
        self.reject_flag("skip_snapshot_tests", self.skip_snapshot_tests)?;
        self.reject_flag("update_all", self.update_all)?;
        self.reject_flag("interactive", self.interactive)?;
        self.reject_present("test_dir", self.test_dir.as_deref())?;
        self.reject_present("snapshot_dir", self.snapshot_dir.as_deref())?;
        self.reject_flag("include_off", self.include_off)?;
        self.reject_nth_child()?;
        self.reject_range()?;
        self.reject_composite_rules()?;
        self.reject_transform()?;

        // Validate format value — only scan workflow uses this parameter.
        if let Some(fmt) = self.effective_format()
            && !VALID_FORMAT_VALUES.contains(&fmt)
        {
            bail!(
                "invalid `format` value `{}`; expected one of: {}",
                fmt,
                VALID_FORMAT_VALUES.join(", ")
            );
        }

        // Validate report_style value — only scan workflow uses this parameter.
        if let Some(style) = self.effective_report_style()
            && !VALID_REPORT_STYLE_VALUES.contains(&style)
        {
            bail!(
                "invalid `report_style` value `{}`; expected one of: {}",
                style,
                VALID_REPORT_STYLE_VALUES.join(", ")
            );
        }

        // Validate builtin_rules values — only scan workflow uses this parameter.
        if let Some(rules) = self.effective_builtin_rules() {
            for rule in rules {
                let rule_name = rule.split(':').next().unwrap_or(rule);
                if !VALID_BUILTIN_RULES.contains(&rule_name) {
                    bail!(
                        "invalid builtin rule `{rule_name}`; expected one of: {}",
                        VALID_BUILTIN_RULES.join(", ")
                    );
                }
            }
        }

        Ok(())
    }

    pub(super) fn validate_test(&self) -> Result<()> {
        self.reject_present("pattern", self.pattern.as_deref())?;
        self.reject_present("kind", self.kind.as_deref())?;
        self.reject_present("path", self.path.as_deref())?;
        self.reject_present("lang", self.lang.as_deref())?;
        self.reject_present("selector", self.selector.as_deref())?;
        self.reject_present(
            "strictness",
            self.strictness.as_ref().map(StructuralStrictness::as_str),
        )?;
        self.reject_present(
            "debug_query",
            self.debug_query.as_ref().map(DebugQueryFormat::as_str),
        )?;
        self.reject_present("regex", self.regex.as_deref())?;
        self.reject_nth_child()?;
        self.reject_range()?;
        self.reject_composite_rules()?;
        self.reject_transform()?;
        if self.globs.is_some() {
            bail!(
                "action='structural' workflow='test' does not accept `globs`; use `config_path`, `filter`, and `skip_snapshot_tests`."
            );
        }
        if self.context_lines.is_some() {
            bail!(
                "action='structural' workflow='test' does not accept `context_lines`; use `config_path`, `filter`, and `skip_snapshot_tests`."
            );
        }
        if self.max_results.is_some() {
            bail!(
                "action='structural' workflow='test' does not accept `max_results`; use `config_path`, `filter`, and `skip_snapshot_tests`."
            );
        }
        Ok(())
    }

    pub(super) fn validate_inspect(&self) -> Result<()> {
        self.reject_present("pattern", self.pattern.as_deref())?;
        self.reject_present("kind", self.kind.as_deref())?;
        self.reject_present("lang", self.lang.as_deref())?;
        self.reject_present("selector", self.selector.as_deref())?;
        self.reject_present(
            "strictness",
            self.strictness.as_ref().map(StructuralStrictness::as_str),
        )?;
        self.reject_present(
            "debug_query",
            self.debug_query.as_ref().map(DebugQueryFormat::as_str),
        )?;
        self.reject_present("filter", self.filter.as_deref())?;
        self.reject_present("regex", self.regex.as_deref())?;
        self.reject_flag("skip_snapshot_tests", self.skip_snapshot_tests)?;
        self.reject_flag("update_all", self.update_all)?;
        self.reject_flag("interactive", self.interactive)?;
        self.reject_present("test_dir", self.test_dir.as_deref())?;
        self.reject_present("snapshot_dir", self.snapshot_dir.as_deref())?;
        self.reject_flag("include_off", self.include_off)?;
        self.reject_nth_child()?;
        self.reject_range()?;
        self.reject_composite_rules()?;
        self.reject_transform()?;
        if self.globs.is_some() {
            bail!(
                "action='structural' workflow='inspect' does not accept `globs`; use `config_path` and `path`."
            );
        }
        if self.context_lines.is_some() {
            bail!(
                "action='structural' workflow='inspect' does not accept `context_lines`; use `config_path` and `path`."
            );
        }
        if self.max_results.is_some() {
            bail!(
                "action='structural' workflow='inspect' does not accept `max_results`; use `config_path` and `path`."
            );
        }
        Ok(())
    }

    pub(super) fn validate_rewrite(&self) -> Result<()> {
        self.validate_rewrite_or_apply()
    }

    pub(super) fn validate_new(&self) -> Result<()> {
        let subcommand = self
            .new_subcommand
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let subcommand = subcommand.ok_or_else(|| {
            anyhow!(
                "action='structural' workflow='new' requires `new_subcommand` \
                 (one of: project, rule, test, util)"
            )
        })?;

        if !matches!(subcommand, "project" | "rule" | "test" | "util") {
            bail!(
                "action='structural' workflow='new' `new_subcommand` must be one of \
                 project, rule, test, util; got `{subcommand}`"
            );
        }

        // rule, test, and util require a name.
        if subcommand != "project" {
            let name = self
                .new_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            if name.is_none() {
                bail!(
                    "action='structural' workflow='new' subcommand `{subcommand}` \
                     requires `new_name`"
                );
            }
        }

        // rule and util require a language.
        if (subcommand == "rule" || subcommand == "util")
            && self.lang.as_deref().is_none_or(str::is_empty)
        {
            bail!(
                "action='structural' workflow='new' subcommand `{subcommand}` \
                 requires `lang`"
            );
        }

        // Reject fields that don't apply to the new workflow.
        self.reject_present("pattern", self.pattern.as_deref())?;
        self.reject_present("kind", self.kind.as_deref())?;
        self.reject_present("selector", self.selector.as_deref())?;
        self.reject_present(
            "strictness",
            self.strictness.as_ref().map(StructuralStrictness::as_str),
        )?;
        self.reject_present(
            "debug_query",
            self.debug_query.as_ref().map(DebugQueryFormat::as_str),
        )?;
        self.reject_present("filter", self.filter.as_deref())?;
        self.reject_present("regex", self.regex.as_deref())?;
        self.reject_present("rewrite", self.rewrite.as_deref())?;
        self.reject_flag("skip_snapshot_tests", self.skip_snapshot_tests)?;
        self.reject_flag("update_all", self.update_all)?;
        self.reject_flag("interactive", self.interactive)?;
        self.reject_nth_child()?;
        self.reject_range()?;
        self.reject_relational_rules()?;
        self.reject_composite_rules()?;
        self.reject_transform()?;

        Ok(())
    }

    pub(super) fn validate_apply(&self) -> Result<()> {
        self.validate_rewrite_or_apply()
    }

    /// Shared validation for both `rewrite` and `apply` workflows, which
    /// have identical constraints.
    pub(super) fn validate_rewrite_or_apply(&self) -> Result<()> {
        let wf = self.workflow.as_str();

        if self.pattern().is_none() && self.regex_pattern().is_none() {
            bail!("action='structural' workflow='{wf}' requires a non-empty `pattern` or `regex`");
        }

        let has_string_rewrite = self.rewrite_text().is_some();
        let has_fix_config = self.fix_config.is_some();

        if !has_string_rewrite && !has_fix_config {
            bail!(
                "action='structural' workflow='{wf}' requires a non-empty `rewrite` string \
                 or a `fix_config` object with `template` and optional `expand_start`/`expand_end`"
            );
        }

        if has_fix_config {
            self.fix_config
                .as_ref()
                .ok_or_else(|| anyhow!("fix_config must be present after validation"))?
                .validate()?;
        }

        self.reject_present("config_path", self.config_path.as_deref())?;
        self.reject_present("filter", self.filter.as_deref())?;
        self.reject_flag("skip_snapshot_tests", self.skip_snapshot_tests)?;
        self.reject_flag("update_all", self.update_all)?;
        self.reject_flag("interactive", self.interactive)?;
        self.reject_nth_child()?;
        self.reject_range()?;
        self.reject_relational_rules()?;
        self.reject_composite_rules()?;

        if self.debug_query.is_some() && self.lang.as_deref().is_none_or(str::is_empty) {
            bail!(DEBUG_QUERY_LANG_HINT);
        }

        if self.regex_pattern().is_some() && self.lang.as_deref().is_none_or(str::is_empty) {
            bail!("action='structural' with `regex` requires `lang` to be set");
        }

        Ok(())
    }

    pub(super) fn reject_present(&self, field: &str, value: Option<&str>) -> Result<()> {
        if value.is_some_and(|value| !value.trim().is_empty()) {
            bail!(
                "action='structural' workflow='{}' does not accept `{field}`.",
                self.workflow.as_str()
            );
        }
        Ok(())
    }

    pub(super) fn reject_flag(&self, field: &str, value: Option<bool>) -> Result<()> {
        if value.is_some() {
            bail!(
                "action='structural' workflow='{}' does not accept `{field}`.",
                self.workflow.as_str()
            );
        }
        Ok(())
    }

    pub(super) fn reject_nth_child(&self) -> Result<()> {
        if self.nth_child.is_some() {
            bail!(
                "action='structural' workflow='{}' does not accept `nth_child`.",
                self.workflow.as_str()
            );
        }
        Ok(())
    }

    pub(super) fn reject_range(&self) -> Result<()> {
        if self.range.is_some() {
            bail!(
                "action='structural' workflow='{}' does not accept `range`.",
                self.workflow.as_str()
            );
        }
        Ok(())
    }

    pub(super) fn reject_relational_rules(&self) -> Result<()> {
        if self.has.is_some()
            || self.inside.is_some()
            || self.follows.is_some()
            || self.precedes.is_some()
            || self.constraints.is_some()
        {
            bail!(
                "action='structural' workflow='{}' does not accept relational rules (`has`, `inside`, `follows`, `precedes`) or `constraints`.",
                self.workflow.as_str()
            );
        }
        Ok(())
    }

    pub(super) fn reject_composite_rules(&self) -> Result<()> {
        if self.matches.is_some()
            || self.all.is_some()
            || self.any.is_some()
            || self.not.is_some()
            || self.utils.is_some()
        {
            bail!(
                "action='structural' workflow='{}' does not accept composite rules \
                 (`matches`, `all`, `any`, `not`) or `utils`.",
                self.workflow.as_str()
            );
        }
        Ok(())
    }

    pub(super) fn reject_transform(&self) -> Result<()> {
        if self.transform.is_some() {
            bail!(
                "action='structural' workflow='{}' does not accept `transform`; \
                 `transform` is only valid for `query`, `count`, and `rewrite` workflows \
                 that use YAML rule generation.",
                self.workflow.as_str()
            );
        }
        Ok(())
    }

    pub(super) fn requested_path(&self) -> &str {
        self.path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .unwrap_or(".")
    }

    pub(super) fn requested_config_path(&self) -> &str {
        self.config_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .unwrap_or(DEFAULT_AST_GREP_CONFIG_PATH)
    }

    pub(super) fn pattern(&self) -> Option<&str> {
        self.pattern
            .as_deref()
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty())
    }

    pub(super) fn kind(&self) -> Option<&str> {
        self.kind
            .as_deref()
            .map(str::trim)
            .filter(|kind| !kind.is_empty())
    }

    pub(super) fn regex_pattern(&self) -> Option<&str> {
        self.regex
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty())
    }

    pub(super) fn filter(&self) -> Option<&str> {
        self.filter
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub(super) fn rewrite_text(&self) -> Option<&str> {
        self.rewrite
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    /// Returns the effective rewrite template: the simple `rewrite` string,
    /// or the `fix_config.template` when a FixConfig is present.
    pub(super) fn effective_rewrite_template(&self) -> Option<&str> {
        if let Some(rewrite) = self.rewrite_text() {
            return Some(rewrite);
        }
        self.fix_config
            .as_ref()
            .map(|fc| fc.template.trim())
            .filter(|t| !t.is_empty())
    }

    /// Returns `true` when the rewrite must go through the YAML rule path
    /// because a FixConfig with expansion or a transform is present.
    pub(super) fn needs_yaml_rewrite(&self) -> bool {
        self.fix_config
            .as_ref()
            .is_some_and(|fc| fc.has_expansion())
            || self.transform.is_some()
    }

    pub(super) fn normalized_globs(&self) -> Vec<String> {
        let mut result: Vec<String> = self
            .globs
            .clone()
            .map(GlobInput::into_vec)
            .unwrap_or_default()
            .into_iter()
            .map(|glob| glob.trim().to_string())
            .filter(|glob| !glob.is_empty())
            .collect();

        // Merge exclude patterns as negative globs (prefixed with `!`).
        if let Some(excludes) = &self.exclude {
            for pattern in excludes.clone().into_vec() {
                let trimmed = pattern.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }
                // Strip a leading `!` if the caller already provided one,
                // then re-add it to guarantee the negative-glob prefix.
                let negative = if let Some(stripped) = trimmed.strip_prefix('!') {
                    format!("!{stripped}")
                } else {
                    format!("!{trimmed}")
                };
                result.push(negative);
            }
        }

        result
    }

    pub(super) fn effective_max_results(&self) -> usize {
        self.max_results
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .clamp(1, MAX_ALLOWED_RESULTS)
    }

    pub(super) fn normalized_or_inferred_lang(&self) -> Option<String> {
        if let Some(lang) = self
            .lang
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(
                AstGrepLanguage::from_user_value(lang)
                    .map(|language| language.as_str().to_string())
                    .unwrap_or_else(|| lang.to_string()),
            );
        }

        if let Some(language) = AstGrepLanguage::infer_from_path_str(self.requested_path()) {
            return Some(language.as_str().to_string());
        }

        let inferred = match self.globs.as_ref() {
            Some(GlobInput::Single(glob)) => {
                AstGrepLanguage::infer_from_positive_globs([glob.as_str()])
            }
            Some(GlobInput::Multiple(globs)) => {
                AstGrepLanguage::infer_from_positive_globs(globs.iter().map(String::as_str))
            }
            None => None,
        };

        inferred.map(|language| language.as_str().to_string())
    }

    pub(super) fn effective_severities(&self) -> Option<Vec<&str>> {
        self.severities.as_ref().map(|v| {
            v.iter()
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .map(|s| match s.as_str() {
                    "error" => "error",
                    "warning" | "warn" => "warning",
                    "info" => "info",
                    "hint" => "hint",
                    _ => "unknown",
                })
                .collect()
        })
    }

    pub(super) fn effective_no_ignore(&self) -> Option<&[String]> {
        self.no_ignore
            .as_ref()
            .filter(|v| !v.is_empty())
            .map(|v| v.as_slice())
    }

    pub(super) fn effective_follow(&self) -> bool {
        self.follow == Some(true)
    }

    pub(super) fn effective_threads(&self) -> Option<u32> {
        self.threads.map(|t| t.min(MAX_THREADS))
    }

    pub(super) fn effective_format(&self) -> Option<&str> {
        self.format
            .as_deref()
            .map(str::trim)
            .filter(|f| !f.is_empty())
    }

    pub(super) fn effective_report_style(&self) -> Option<&str> {
        self.report_style
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty())
    }

    pub(super) fn effective_before_lines(&self) -> Option<usize> {
        self.before_lines
            .filter(|&n| n <= MAX_ALLOWED_CONTEXT_LINES)
    }

    pub(super) fn effective_after_lines(&self) -> Option<usize> {
        self.after_lines.filter(|&n| n <= MAX_ALLOWED_CONTEXT_LINES)
    }

    pub(super) fn effective_builtin_rules(&self) -> Option<&[String]> {
        self.builtin_rules
            .as_ref()
            .filter(|v| !v.is_empty())
            .map(|v| v.as_slice())
    }
}
