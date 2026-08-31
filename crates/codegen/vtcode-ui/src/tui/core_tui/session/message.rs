use ratatui::text::Line;
use std::sync::Arc;

use crate::tui::ui::tui::types::{InlineLinkRange, InlineLinkTarget, InlineMessageKind, InlineSegment};

#[derive(Clone)]
pub struct MessageLine {
    pub kind: InlineMessageKind,
    pub segments: Vec<InlineSegment>,
    pub link_ranges: Vec<InlineLinkRange>,
    pub revision: u64,
    /// Complete PTY output used by the transcript-review overlay. The live
    /// message remains bounded; this sidecar preserves the full capture.
    pub pty_transcript: Option<Arc<Vec<String>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedTranscriptLink {
    pub start: usize,
    pub end: usize,
    pub start_col: usize,
    pub width: usize,
    pub target: InlineLinkTarget,
}

#[derive(Clone, Debug, Default)]
pub struct TranscriptLine {
    pub line: Line<'static>,
    pub explicit_links: Vec<RenderedTranscriptLink>,
}

#[derive(Clone, Default)]
pub struct MessageLabels {
    pub agent: Option<String>,
    pub user: Option<String>,
}
