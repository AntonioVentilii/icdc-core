use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use shared::types::{AssetId, DecimalValue};

use super::errors::RiskError;

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
risk_result!(RiskChangeResult, RiskChangeView);
risk_result!(VolatilityResult, VolatilityView);
risk_result!(PriceHistoryResult, Vec<PricePointView>);
risk_result!(RiskGovernanceResult, RiskGovernanceView);
risk_result!(UnderlyingRiskParamsResult, Vec<UnderlyingRiskParamsView>);
risk_result!(CalendarEventsResult, Vec<CalendarEventView>);
risk_result!(PendingActionsResult, Vec<PendingActionView>);

/// Whether a requested change took effect or waits behind the timelock.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub enum RiskChangeView {
    Applied,
    Pending {
        action_id: u64,
        executable_at_ns: u64,
    },
}

/// One entry of the public risk audit log.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct RiskAuditEntryView {
    pub seq: u64,
    pub time_ns: u64,
    pub caller: Principal,
    pub description: String,
}

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
    LowerRiskMultiplier {
        underlying: String,
        from: DecimalValue,
        to: DecimalValue,
    },
    LowerGlobalRiskMultiplier {
        from: DecimalValue,
        to: DecimalValue,
    },
    ChangeTimelockDuration {
        from_ns: u64,
        to_ns: u64,
    },
    ActivateHistoryUpload {
        upload_id: u64,
    },
    RemoveHistoryPoints {
        underlying: String,
        from_date: u32,
        to_date: u32,
        expected_sha256: String,
    },
    AddRiskAdmin {
        principal: Principal,
    },
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
    /// Long-run volatility over the whole history. Absent while the estimate is provisional.
    pub sigma_long_run: Option<DecimalValue>,
    /// `max(sigma_ewma, floor_ratio x sigma_long_run)`. Absent while the estimate is provisional.
    pub sigma_used: Option<DecimalValue>,
    /// True while the history holds fewer than 250 returns: the EWMA is published, but there is no
    /// long-run volatility and no floor yet.
    pub is_provisional: bool,
    pub returns: u32,
    pub last_date: u32,
    pub mpor_days: u32,
    /// Volatility time over the next `mpor_days` days, including weekends and calendar events.
    pub mpor_volatility_time: DecimalValue,
    /// `k x global_k`.
    pub k_effective: DecimalValue,
}
