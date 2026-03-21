use candid::CandidType;
use serde::{Deserialize, Serialize};

use crate::types::errors::CommonError;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum MigrationError {
    Common(CommonError),
    SameDomain,
    InFlightPlansExist,
    NoStateTOMigrate,
}
