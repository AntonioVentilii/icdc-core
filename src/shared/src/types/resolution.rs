use candid::CandidType;
use serde::{Deserialize, Serialize};

/// Settlement terms for a series. Compulsory on every market.
///
/// Modeled as a struct (not a bare `String`) so it can grow without a breaking
/// candid change: future fields — e.g. structured `rules` (named source +
/// settle date + day-count, see issue #64) — are added as `opt` fields, which
/// candid can decode against records persisted before the field existed.
/// Variants were considered and rejected: adding an enum variant risks decode
/// failures in already-deployed consumers, whereas appending `opt` record
/// fields is forward-compatible.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    /// Human-readable clause describing how the market settles.
    pub clause: String,
    // Future (#64): pub rules: Option<ResolutionRules>,
}

impl Resolution {
    /// Creates a new resolution from a clause string.
    pub fn new(clause: impl Into<String>) -> Self {
        Self {
            clause: clause.into(),
        }
    }
}

impl From<String> for Resolution {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for Resolution {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}
