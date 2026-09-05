use serde::{Deserialize, Serialize};

/// Structured input content used by the TUI input/history systems.
///
/// `Image` data is boxed (base64 payloads dwarf every other variant) so a
/// `Vec<ContentPart>` of text parts does not pay the inline footprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    Image { data: Box<str>, media_type: String },
}

impl ContentPart {
    pub(crate) fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn image(data: impl Into<String>, media_type: impl Into<String>) -> Self {
        Self::Image {
            data: data.into().into_boxed_str(),
            media_type: media_type.into(),
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image { .. })
    }
}
