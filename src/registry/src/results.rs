use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::{Series, SeriesId};

use crate::errors::{OracleError, SeriesError};

/// The result of an [`add_series`] operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum AddSeriesResult {
    /// Successfully registered the series with the returned [`SeriesId`].
    Ok(SeriesId),
    /// Failed to register the series.
    Err(SeriesError),
}
impl From<Result<SeriesId, SeriesError>> for AddSeriesResult {
    fn from(value: Result<SeriesId, SeriesError>) -> Self {
        match value {
            Ok(v) => AddSeriesResult::Ok(v),
            Err(e) => AddSeriesResult::Err(e),
        }
    }
}

/// A paginated page of registered series.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SeriesPage {
    /// The list of series in this page.
    pub items: Vec<Series>,
    /// The cursor to be used for the next request, if any.
    pub next_cursor: Option<SeriesId>,
}

/// The result of an oracle-related operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum OracleResult {
    Ok,
    Err(OracleError),
}
impl From<Result<(), OracleError>> for OracleResult {
    fn from(value: Result<(), OracleError>) -> Self {
        match value {
            Ok(()) => OracleResult::Ok,
            Err(e) => OracleResult::Err(e),
        }
    }
}
