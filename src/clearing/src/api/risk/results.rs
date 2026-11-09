use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use shared::types::{AssetId, DecimalValue};

use super::errors::RiskError;
use crate::risk::state::RiskChangeOutcome;

macro_rules! risk_result {
    ($name:ident, $ok:ty) => {
        #[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
        pub enum $name {
            Ok($ok),
            Err(RiskError),
        }
        impl From<Result<$ok, RiskError>> for $name {
            fn from(res: Result<$ok, RiskError>) -> Self {
                match res {
                    Ok(v) => Self::Ok(v),
                    Err(e) => Self::Err(e),
                }
            }
        }
    };
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum RiskUnitResult {
    Ok,
    Err(RiskError),
}
impl From<Result<(), RiskError>> for RiskUnitResult {
    fn from(res: Result<(), RiskError>) -> Self {
        match res {
            Ok(()) => Self::Ok,
            Err(e) => Self::Err(e),
        }
    }
}

risk_result!(RiskIdResult, u64);
risk_result!(RiskIdsResult, Vec<u64>);
risk_result!(RiskCountResult, u32);
risk_result!(RiskChangeResult, RiskChangeOutcome);
risk_result!(VolatilityResult, VolatilityView);
risk_result!(PriceHistoryResult, Vec<PricePointView>);

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct RiskGovernanceView {
    pub risk_admins: Vec<Principal>,
    pub global_k: DecimalValue,
    pub timelock_duration_ns: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct UnderlyingRiskParamsView {
    pub underlying: String,
    pub lambda: DecimalValue,
    pub weekend_weight: DecimalValue,
    pub floor_ratio: DecimalValue,
    pub mpor_days: u32,
    pub k: DecimalValue,
    pub oracle_asset_id: Option<AssetId>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CalendarEventView {
    pub id: u64,
    pub time_ns: u64,
    pub weight: DecimalValue,
    pub label: String,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum PendingActionKindView {
    LowerRiskMultiplier { underlying: String, k: DecimalValue },
    LowerGlobalRiskMultiplier { k: DecimalValue },
    ActivateHistoryUpload { upload_id: u64 },
    AddRiskAdmin { principal: Principal },
    LowerTimelockDuration { duration_ns: u64 },
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct PendingActionView {
    pub id: u64,
    pub action: PendingActionKindView,
    pub proposed_by: Principal,
    pub proposed_at_ns: u64,
    pub executable_at_ns: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum HistoryUploadStatusView {
    Open,
    Sealed { action_id: u64 },
    Activated { activated_at_ns: u64 },
    Cancelled,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct HistoryUploadView {
    pub id: u64,
    pub underlying: String,
    pub source: String,
    pub series_id: String,
    pub point_count: u32,
    pub first_date: Option<u32>,
    pub last_date: Option<u32>,
    pub sha256: Option<String>,
    pub status: HistoryUploadStatusView,
    pub created_by: Principal,
    pub created_at_ns: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct PricePointView {
    pub date: u32,
    pub price: DecimalValue,
}

/// Daily volatility per unit of volatility time.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct VolatilityView {
    pub underlying: String,
    pub sigma_ewma: DecimalValue,
    pub sigma_long_run: DecimalValue,
    /// `max(sigma_ewma, floor_ratio x sigma_long_run)`.
    pub sigma_used: DecimalValue,
    pub returns: u32,
    pub last_date: u32,
    pub mpor_days: u32,
    /// Volatility time over the next `mpor_days` days, including weekends and calendar events.
    pub mpor_volatility_time: DecimalValue,
    /// `k x global_k`.
    pub k_effective: DecimalValue,
}
