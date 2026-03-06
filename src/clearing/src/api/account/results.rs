use candid::{CandidType, Deserialize};
use serde::Serialize;

use super::errors::MarginAccountError;
use crate::types::margin::MarginAccount;

/// Result of a margin account retrieval request.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum GetMarginAccountResult {
    /// Successfully retrieved the margin account details.
    Ok(MarginAccount),
    /// Failed to retrieve the margin account.
    Err(MarginAccountError),
}
impl From<Result<MarginAccount, MarginAccountError>> for GetMarginAccountResult {
    fn from(value: Result<MarginAccount, MarginAccountError>) -> Self {
        match value {
            Ok(v) => GetMarginAccountResult::Ok(v),
            Err(e) => GetMarginAccountResult::Err(e),
        }
    }
}
