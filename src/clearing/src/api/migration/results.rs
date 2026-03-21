use candid::CandidType;
use serde::{Deserialize, Serialize};

use super::errors::MigrationError;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum MigrateDomainResult {
    Ok,
    Err(MigrationError),
}

impl From<Result<(), MigrationError>> for MigrateDomainResult {
    fn from(res: Result<(), MigrationError>) -> Self {
        match res {
            Ok(()) => Self::Ok,
            Err(e) => Self::Err(e),
        }
    }
}
