use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::DecimalValue;

/// Errors of the risk endpoints.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum RiskError {
    /// A value lies outside the hard bounds fixed in code.
    OutOfBounds {
        parameter: String,
        min: DecimalValue,
        max: DecimalValue,
    },
    /// A parameter is malformed or out of its allowed range.
    InvalidParameter { parameter: String, reason: String },
    /// The underlying identifier is malformed.
    InvalidUnderlying(String),
    /// No risk parameters exist for this underlying yet.
    UnknownUnderlying(String),
    /// A fixed-point computation failed (overflow, division by zero, logarithm of a non-positive
    /// value).
    ArithmeticError(String),
    /// No history upload with this id.
    UploadNotFound(u64),
    /// The upload no longer accepts chunks or sealing.
    UploadNotOpen(u64),
    /// A history chunk was rejected.
    InvalidHistoryChunk(String),
    /// The declared SHA-256 differs from the hash of the uploaded points.
    HashMismatch { computed: String },
    /// No pending action with this id.
    PendingActionNotFound(u64),
    /// The action cannot run before this time.
    TimelockNotExpired { executable_at_ns: u64 },
    /// The action no longer applies to the current state.
    ActionNoLongerValid(String),
    /// A storage limit was reached.
    LimitReached(String),
    /// Not enough history to estimate volatility.
    InsufficientHistory { returns: u32, required: u32 },
    /// The principal cannot take this role.
    InvalidPrincipal(String),
    /// No calendar event with this id for the underlying.
    CalendarEventNotFound(u64),
}
