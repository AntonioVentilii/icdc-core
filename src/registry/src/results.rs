use candid::{CandidType, Deserialize};
use serde::Serialize;

use crate::error::RegistryError;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum AddSeriesResult {
    Ok(String),
    Err(RegistryError),
}
impl From<Result<String, RegistryError>> for AddSeriesResult {
    fn from(value: Result<String, RegistryError>) -> Self {
        match value {
            Ok(v) => AddSeriesResult::Ok(v),
            Err(e) => AddSeriesResult::Err(e),
        }
    }
}
