#![allow(unused_imports)]

#[allow(unused_imports)]
use super::*;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AstGrepMetaVar {
    pub(super) text: String,
    pub(super) range: AstGrepRange,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct AstGrepMetaVariables {
    #[serde(default)]
    pub(super) single: BTreeMap<String, AstGrepMetaVar>,
    #[serde(default)]
    pub(super) multi: BTreeMap<String, Vec<AstGrepMetaVar>>,
    #[serde(default)]
    pub(super) transformed: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AstGrepMatch {
    pub(super) file: String,
    pub(super) text: String,
    #[serde(default)]
    pub(super) lines: Option<String>,
    #[serde(default)]
    pub(super) language: Option<String>,
    pub(super) range: AstGrepRange,
    #[serde(default, rename = "metaVariables")]
    pub(super) meta_variables: Option<AstGrepMetaVariables>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AstGrepRewriteMatch {
    pub(super) file: String,
    pub(super) text: String,
    #[serde(default)]
    pub(super) lines: Option<String>,
    #[serde(default)]
    pub(super) language: Option<String>,
    pub(super) range: AstGrepRange,
    #[serde(default, rename = "metaVariables")]
    pub(super) meta_variables: Option<AstGrepMetaVariables>,
    #[serde(default)]
    pub(super) replacement: Option<String>,
    #[serde(default, rename = "replacementOffsets")]
    pub(super) replacement_offsets: Option<AstGrepByteOffset>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AstGrepLabel {
    pub(super) text: String,
    pub(super) range: AstGrepRange,
    #[serde(default)]
    pub(super) source: Option<String>,
}

/// Severity level for ast-grep scan findings.
///
/// ast-grep defines five severity levels:
/// - `error`: reports an error; causes `ast-grep scan` to exit non-zero
/// - `warning`: reports a warning
/// - `info`: reports an informational message
/// - `hint`: reports a hint (the default severity for ast-grep rules)
/// - `off`: disables the rule entirely
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AstGrepSeverity {
    Error,
    Warning,
    Info,
    Hint,
    Off,
}

impl AstGrepSeverity {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
            Self::Off => "off",
        }
    }

    pub(super) fn from_str_normalized(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warning" | "warn" => Some(Self::Warning),
            "info" => Some(Self::Info),
            "hint" => Some(Self::Hint),
            "off" | "none" => Some(Self::Off),
            _ => None,
        }
    }
}

impl fmt::Display for AstGrepSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AstGrepSeverity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        AstGrepSeverity::from_str_normalized(&s).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unknown severity `{s}`; expected error, warning, info, hint, or off"
            ))
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AstGrepScanFinding {
    pub(super) file: String,
    pub(super) text: String,
    #[serde(default)]
    pub(super) lines: Option<String>,
    #[serde(default)]
    pub(super) language: Option<String>,
    pub(super) range: AstGrepRange,
    #[serde(default, rename = "ruleId")]
    pub(super) rule_id: Option<String>,
    #[serde(default)]
    pub(super) severity: Option<AstGrepSeverity>,
    #[serde(default)]
    pub(super) message: Option<String>,
    #[serde(default)]
    pub(super) note: Option<String>,
    #[serde(default)]
    pub(super) metadata: Option<Value>,
    #[serde(default)]
    pub(super) labels: Vec<AstGrepLabel>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AstGrepByteOffset {
    pub(super) start: usize,
    pub(super) end: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AstGrepRange {
    pub(super) start: AstGrepPoint,
    pub(super) end: AstGrepPoint,
    #[serde(default, rename = "byteOffset")]
    pub(super) byte_offset: Option<AstGrepByteOffset>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AstGrepPoint {
    pub(super) line: usize,
    pub(super) column: usize,
}
