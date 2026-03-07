use candid::CandidType;
use serde::{Deserialize, Serialize};

/// A multiformat description.
/// Provides multiple versions of the same content for different rendering environments.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Description {
    /// The mandatory plain text description.
    pub plain: String,
    /// Optional markdown version of the description.
    pub markdown: Option<String>,
    /// Optional HTML version of the description.
    pub html: Option<String>,
}

impl Description {
    /// Creates a new plain-text-only description.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            plain: text.into(),
            ..Default::default()
        }
    }

    /// Creates a new description with both plain text and markdown.
    pub fn markdown(plain: impl Into<String>, markdown: impl Into<String>) -> Self {
        Self {
            plain: plain.into(),
            markdown: Some(markdown.into()),
            ..Default::default()
        }
    }

    /// Creates a new description with plain text, markdown, and HTML.
    pub fn all(
        plain: impl Into<String>,
        markdown: impl Into<String>,
        html: impl Into<String>,
    ) -> Self {
        Self {
            plain: plain.into(),
            markdown: Some(markdown.into()),
            html: Some(html.into()),
        }
    }
}

impl From<String> for Description {
    fn from(value: String) -> Self {
        Self::plain(value)
    }
}

impl From<&str> for Description {
    fn from(value: &str) -> Self {
        Self::plain(value)
    }
}
