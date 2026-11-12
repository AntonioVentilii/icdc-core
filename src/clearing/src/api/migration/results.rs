use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::impl_from_result;

use super::errors::MigrationError;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum MigrateDomainResult {
    Ok,
    Err(MigrationError),
}

impl_from_result!(MigrateDomainResult, (), MigrationError);
