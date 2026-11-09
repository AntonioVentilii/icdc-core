use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use shared::types::{AssetId, DecimalValue};

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SetUnderlyingRiskParamsParams {
    /// Underlying identifier, for example `BTC` or `EUR`. Canonicalized to upper case.
    pub underlying: String,
    pub lambda: DecimalValue,
    pub weekend_weight: DecimalValue,
    pub floor_ratio: DecimalValue,
    pub mpor_days: u32,
    /// Collateral asset whose oracle price feeds the daily history, if any.
    pub oracle_asset_id: Option<AssetId>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SetRiskMultiplierParams {
    pub underlying: String,
    pub k: DecimalValue,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SetGlobalRiskMultiplierParams {
    pub k: DecimalValue,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SetTimelockDurationParams {
    pub duration_ns: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct NewCalendarEvent {
    pub time_ns: u64,
    /// Variance of the event's day as a multiple of an ordinary day.
    pub weight: DecimalValue,
    pub label: String,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AddCalendarEventsParams {
    pub underlying: String,
    pub events: Vec<NewCalendarEvent>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct RemoveCalendarEventParams {
    pub underlying: String,
    pub event_id: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct BeginHistoryUploadParams {
    pub underlying: String,
    /// Where the data comes from, for example `FRED` or `Coinbase`.
    pub source: String,
    /// The source's identifier of the series, for example `DEXUSEU`.
    pub series_id: String,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct PricePointInput {
    /// Days since the Unix epoch, UTC.
    pub date: u32,
    /// USD per unit of the underlying.
    pub price: DecimalValue,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AppendHistoryChunkParams {
    pub upload_id: u64,
    pub points: Vec<PricePointInput>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct SealHistoryUploadParams {
    pub upload_id: u64,
    /// Hex SHA-256 of the canonical encoding (see `docs/plans/margin-phase-1-implementation.md`,
    /// section 1.3).
    pub sha256_hex: String,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct GetPriceHistoryParams {
    pub underlying: String,
    /// First date to return (inclusive). Defaults to the start of the history.
    pub from_date: Option<u32>,
    /// Maximum number of points. Defaults to 1,000, capped at 5,000.
    pub limit: Option<u32>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct ListRiskAuditParams {
    /// First sequence number to return (inclusive). Defaults to 0.
    pub from_seq: Option<u64>,
    /// Maximum number of entries. Defaults to 100, capped at 1,000.
    pub limit: Option<u32>,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct RiskAdminParams {
    pub principal: Principal,
}
