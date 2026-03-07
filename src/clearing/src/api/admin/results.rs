use std::collections::BTreeMap;

use candid::CandidType;
use serde::{Deserialize, Serialize};
use shared::types::Asset;

use crate::types::errors::CommonError;

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetFundsResult {
    pub insurance_fund: BTreeMap<Asset, u128>,
    pub treasury: BTreeMap<Asset, u128>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum AdminResult<T> {
    Ok(T),
    Err(AdminError),
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum AdminError {
    Common(CommonError),
    InsufficientFunds,
    TransferFailed(String),
}
impl<T> From<Result<T, AdminError>> for AdminResult<T> {
    fn from(res: Result<T, AdminError>) -> Self {
        match res {
            Ok(v) => Self::Ok(v),
            Err(e) => Self::Err(e),
        }
    }
}
