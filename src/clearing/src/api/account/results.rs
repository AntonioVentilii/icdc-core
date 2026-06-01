use candid::{CandidType, Deserialize};
use serde::Serialize;
use shared::types::{AssetId, OutcomeId, SeriesId};

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
    pub available_margin_usd: i128,
}

/// Outcome of an account state retrieval request.
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

/// Aggregate lean of the supplied principal set on a single outcome of a
/// series.
///
/// Carries **counts only** — no principal identities, sides, quantities, or
/// P&L. Long is the number of net-long holders, short the number of net-short
/// holders; flat (zero net) positions are excluded from both.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OutcomeLean {
    /// The outcome this lean is for. `None` is the series' binary payoff (long
    /// vs short); `Some` identifies a categorical outcome.
    pub outcome_id: Option<OutcomeId>,
    /// Number of supplied principals net long on this outcome.
    pub long: u64,
    /// Number of supplied principals net short on this outcome.
    pub short: u64,
    /// `long + short`, the number of supplied principals with a non-flat
    /// position on this outcome.
    pub total: u64,
}

/// Aggregate long/short lean of a supplied set of principals on a series,
/// broken down per outcome.
///
/// Privacy-safe by construction: it exposes only aggregate counts over the
/// supplied set, never individual identities, sides, amounts, or P&L.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AggregateLean {
    /// The series this lean was computed for.
    pub series_id: SeriesId,
    /// Per-outcome aggregate lean, one entry per outcome on which at least one
    /// supplied principal holds a non-flat position, ordered by outcome.
    pub outcomes: Vec<OutcomeLean>,
    /// Number of distinct supplied principals holding a non-flat position
    /// anywhere on the series (a principal long on two outcomes counts once).
    pub total: u64,
}
