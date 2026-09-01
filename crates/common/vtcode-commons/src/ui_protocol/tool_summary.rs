//! Data exchanged by compact per-call tool summaries.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactToolSummaryLine {
    pub kind: CompactToolSummaryLineKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactToolSummaryLineKind {
    Info,
    Detail,
}
