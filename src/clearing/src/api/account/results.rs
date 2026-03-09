use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::AssetId;

use super::errors::AccountStateError;
use crate::types::margin::AccountState;

/// Represents the worth of a specific asset in the user's account.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AssetWorth {
    /// The unique identifier of the asset.
    pub asset_id: AssetId,
    /// The raw balance of the asset.
    pub balance: u128,
    /// The USD value of the balance (including haircut).
    pub value_usd: u128,
    /// The USD value of the balance (before haircut).
    pub pre_haircut_value_usd: u128,
    /// The applied haircut in basis points.
    pub haircut_bps: u16,
}

/// Enhanced response for account state with calculated USD values.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AccountStateResponse {
    /// The raw account state.
    pub state: AccountState,
    /// Detailed worth of each collateral asset.
    pub assets: Vec<AssetWorth>,
    /// Total equity in USD (cash balance + sum of collateral values).
    pub total_equity_usd: u128,
    /// Available equity in USD (total equity - reserved margin).
    pub available_equity_usd: i128,
}

/// Result of an account state retrieval request.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum GetAccountStateResult {
    /// Successfully retrieved the account state details.
    Ok(AccountStateResponse),
    /// Failed to retrieve the account state.
    Err(AccountStateError),
}
impl From<Result<AccountStateResponse, AccountStateError>> for GetAccountStateResult {
    fn from(value: Result<AccountStateResponse, AccountStateError>) -> Self {
        match value {
            Ok(v) => GetAccountStateResult::Ok(v),
            Err(e) => GetAccountStateResult::Err(e),
        }
    }
}
