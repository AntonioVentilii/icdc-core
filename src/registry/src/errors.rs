use candid::CandidType;
use serde::{Deserialize, Serialize};

/// Errors that can occur during series-related operations.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum SeriesError {
    /// Returned when attempting to add a series that already exists.
    SeriesAlreadyExists,
    /// Returned when the provided title exceeds the maximum allowed length.
    TitleTooLong,
    /// Returned when the provided description exceeds the maximum allowed length.
    DescriptionTooLong,
    /// Returned when the caller is not authorized to add a series.
    Unauthorized,
}

/// Errors that can occur during oracle-related operations.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum OracleError {
    /// Returned when attempting to add an oracle that already exists.
    OracleAlreadyExists,
    /// Returned when the specified oracle does not exist.
    OracleNotFound,
    /// Returned when the caller is not authorised to manage the oracle.
    UnauthorizedOracleManager,
}
