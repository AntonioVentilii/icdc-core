use candid::CandidType;
use serde::{Deserialize, Serialize};

/// Errors that can occur during registry operations.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum RegistryError {
    /// Returned when attempting to add a series that already exists in the registry.
    SeriesAlreadyExists,
}
