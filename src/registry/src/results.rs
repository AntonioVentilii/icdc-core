use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::SeriesId;

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
