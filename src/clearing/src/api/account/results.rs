use candid::{CandidType, Deserialize};
use serde::Serialize;

use super::errors::AccountStateError;
use crate::types::margin::AccountState;

/// Result of an account state retrieval request.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum GetAccountStateResult {
    /// Successfully retrieved the account state details.
    Ok(AccountState),
    /// Failed to retrieve the account state.
    Err(AccountStateError),
}
impl From<Result<AccountState, AccountStateError>> for GetAccountStateResult {
    fn from(value: Result<AccountState, AccountStateError>) -> Self {
        match value {
            Ok(v) => GetAccountStateResult::Ok(v),
            Err(e) => GetAccountStateResult::Err(e),
        }
    }
}
