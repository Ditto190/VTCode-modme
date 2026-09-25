#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeFenceBlock {
    pub language: Option<String>,
    pub lines: Vec<String>,
}

/// Classify a line as a fenced-code-block delimiter.
///
/// Returns `Some((fence_char, is_closing))` when the line opens or closes a
/// fence: an opener is 3+ backticks/tildes (info string allowed), a closer is
/// 3+ of the same fence character with nothing but whitespace after. A line
/// with an info string while a fence is open is body text (`None`).
fn fence_delimiter_line(line: &str, open_char: Option<char>) -> Option<(char, bool)> {
    let trimmed = line.trim();
    let mut chars = trimmed.chars();
    let first = chars.next()?;
    if first != '`' && first != '~' {
        return None;
    }
    let mut count = 1usize;
    let mut closed_run = false;
    for ch in chars {
        if ch == first {
            count += 1;
        } else {
            closed_run = true;
            break;
        }
    }
    if count < 3 {
        return None;
    }
    if closed_run {
        // Info string present: only valid as an opener.
        return Some((first, false));
    }
    let only_whitespace = trimmed.chars().skip(count).all(char::is_whitespace);
    match open_char {
        Some(open) if open == first && only_whitespace => Some((first, true)),
        Some(_) => None,
        None => Some((first, false)),
    }
}

/// Byte ranges of `text` outside fenced code blocks.
///
/// Markup inside a fence is quoted documentation (skill docs, test fixtures,
/// echoed file content), not an executable call. Textual tool parsers only
/// consider unfenced regions.
pub(crate) fn unfenced_byte_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut open_char: Option<char> = None;
    let mut segment_start = 0usize;
    let mut cursor = 0usize;
    for line in text.split_inclusive('\n') {
        let line_start = cursor;
        cursor += line.len();
        let Some((fence_char, is_closing)) = fence_delimiter_line(line, open_char) else {
            continue;
        };
        if is_closing {
            open_char = None;
        } else if open_char.is_none() {
            ranges.push(segment_start..line_start);
            open_char = Some(fence_char);
        }
        segment_start = cursor;
    }
    if open_char.is_none() {
        ranges.push(segment_start..text.len());
    }
    ranges.retain(|range| range.start < range.end);
    ranges
}

/// Next byte offset of `needle` at or after `from`, outside fenced code blocks.
pub(crate) fn find_unfenced_from(text: &str, needle: &str, from: usize) -> Option<usize> {
    unfenced_byte_ranges(text).into_iter().find_map(|range| {
        let start = range.start.max(from);
        if start >= range.end {
            return None;
        }
        text.get(start..range.end)
            .and_then(|slice| slice.find(needle))
            .map(|index| start + index)
    })
}

pub(crate) fn extract_code_fence_blocks(text: &str) -> Vec<CodeFenceBlock> {
    // Estimate capacity: assume ~1 code block per 30 lines on average, cap at 20
    let estimated_blocks = text.lines().count() / 30 + 1;
    let mut blocks = Vec::with_capacity(estimated_blocks.min(20));
    let mut current_language: Option<String> = None;

    // Pre-allocate line buffer based on text size estimate
    let estimated_lines = text.lines().count() / 5; // Assume ~20% of lines are code
    let mut current_lines: Vec<String> = Vec::with_capacity(estimated_lines.min(1000)); // Cap at 1000 lines

    for raw_line in text.lines() {
        let trimmed_start = raw_line.trim_start();
        if let Some(rest) = trimmed_start.strip_prefix("```") {
            let rest_clean = rest.trim_matches('\r');
            let rest_trimmed = rest_clean.trim();
            if current_language.is_some() {
                if rest_trimmed.is_empty() {
                    let language = current_language.take().and_then(|lang| {
                        let cleaned = lang.trim_matches(|ch| matches!(ch, '"' | '\'' | '`'));
                        let cleaned = cleaned.trim();
                        if cleaned.is_empty() {
                            None
                        } else {
                            Some(cleaned.to_string())
                        }
                    });
                    let block_lines = std::mem::take(&mut current_lines);
                    blocks.push(CodeFenceBlock { language, lines: block_lines });
                    continue;
                }
            } else {
                let token = rest_trimmed.split_whitespace().next().unwrap_or_default();
                let normalized = token.trim_matches(|ch| matches!(ch, '"' | '\'' | '`')).trim();
                current_language = Some(normalized.to_ascii_lowercase());
                current_lines.clear();
                continue;
            }
        }

        if current_language.is_some() {
            current_lines.push(raw_line.trim_end_matches('\r').to_string());
        }
    }

    blocks
}
