use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::{Series, SeriesId};

use crate::errors::RegistryError;

/// The result of an [`add_series`] operation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum AddSeriesResult {
    /// Successfully registered the series with the returned [`SeriesId`].
    Ok(SeriesId),
    /// Failed to register the series.
    Err(RegistryError),
}
impl From<Result<SeriesId, RegistryError>> for AddSeriesResult {
    fn from(value: Result<SeriesId, RegistryError>) -> Self {
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
