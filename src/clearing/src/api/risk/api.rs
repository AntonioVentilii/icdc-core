//! Risk endpoints: roles, bounded parameters, the timelock, price history and volatility (phase 1
//! of `docs/risk/margin-model.md`). Every query is public; every update is authorized by its guard
//! and audited.

use ic_cdk::api::{is_controller, msg_caller};
use ic_cdk_macros::{query, update};
use shared::types::{AssetId, DecimalValue};

use super::{
    errors::RiskError,
    params::{
        AddCalendarEventsParams, AppendHistoryChunkParams, BeginHistoryUploadParams,
        GetPriceHistoryParams, ListRiskAuditParams, RemoveCalendarEventParams, RiskAdminParams,
        SealHistoryUploadParams, SetGlobalRiskMultiplierParams, SetRiskMultiplierParams,
        SetTimelockDurationParams, SetUnderlyingRiskParamsParams,
    },
    results::{
        CalendarEventView, HistoryUploadStatusView, HistoryUploadView, PendingActionKindView,
        PendingActionView, PriceHistoryResult, PricePointView, RiskChangeResult, RiskCountResult,
        RiskGovernanceView, RiskIdResult, RiskIdsResult, RiskUnitResult, UnderlyingRiskParamsView,
        VolatilityResult, VolatilityView,
    },
};
use crate::{
    guards::{
        caller_is_controller, caller_is_not_anonymous, caller_is_risk_admin,
        caller_is_risk_admin_or_controller,
    },
    memory::RISK_STATE,
    risk::{
        fixed::Fixed,
        history::{PricePoint, UploadStatus},
        params::canonical_underlying,
        state::{NewEvent, RiskAuditEntry, UnderlyingParamsInput},
        timelock::TimelockedAction,
    },
    utils::system::now_ns,
};

const DEFAULT_HISTORY_LIMIT: u32 = 1_000;
const MAX_HISTORY_LIMIT: u32 = 5_000;
const DEFAULT_AUDIT_LIMIT: u32 = 100;
const MAX_AUDIT_LIMIT: u32 = 1_000;

fn fixed(parameter: &str, value: &DecimalValue) -> Result<Fixed, RiskError> {
    Fixed::from_decimal(value).map_err(|e| RiskError::InvalidParameter {
        parameter: parameter.to_owned(),
        reason: e.to_string(),
    })
}

fn underlying_input(
    lambda: &DecimalValue,
    weekend_weight: &DecimalValue,
    floor_ratio: &DecimalValue,
    mpor_days: u32,
    oracle_asset_id: Option<AssetId>,
) -> Result<UnderlyingParamsInput, RiskError> {
    Ok(UnderlyingParamsInput {
        lambda: fixed("lambda", lambda)?,
        weekend_weight: fixed("weekend_weight", weekend_weight)?,
        floor_ratio: fixed("floor_ratio", floor_ratio)?,
        mpor_days,
        oracle_asset_id,
    })
}

/// Risk figures are never negative; a negative value would mean a bug, and is reported as zero.
fn decimal(value: Fixed) -> DecimalValue {
    value.to_decimal().unwrap_or_default()
}

// ----- Roles -----

/// Proposes granting the risk administrator role. Takes effect after the timelock. A canister
/// controller cannot be made a risk administrator.
#[update(guard = "caller_is_controller")]
#[must_use]
pub fn add_risk_admin(params: RiskAdminParams) -> RiskIdResult {
    let RiskAdminParams { principal } = params;
    let target_is_controller = is_controller(&principal);
    RISK_STATE
        .with(|s| {
            s.borrow_mut().propose_risk_admin(
                principal,
                target_is_controller,
                msg_caller(),
                now_ns(),
            )
        })
        .into()
}

/// Removes a risk administrator immediately.
#[update(guard = "caller_is_controller")]
#[must_use]
pub fn remove_risk_admin(params: RiskAdminParams) -> RiskUnitResult {
    let RiskAdminParams { principal } = params;
    RISK_STATE
        .with(|s| {
            s.borrow_mut()
                .remove_risk_admin(principal, msg_caller(), now_ns())
        })
        .into()
}

// ----- Parameters -----

/// Sets the parameters of an underlying other than its multiplier, within hard bounds. Creates the
/// underlying with `k = 1` if it does not exist.
#[update(guard = "caller_is_risk_admin")]
#[must_use]
pub fn set_underlying_risk_params(params: SetUnderlyingRiskParamsParams) -> RiskUnitResult {
    let SetUnderlyingRiskParamsParams {
        underlying,
        lambda,
        weekend_weight,
        floor_ratio,
        mpor_days,
        oracle_asset_id,
    } = params;
    underlying_input(
        &lambda,
        &weekend_weight,
        &floor_ratio,
        mpor_days,
        oracle_asset_id,
    )
    .and_then(|input| {
        RISK_STATE.with(|s| {
            s.borrow_mut()
                .set_underlying_params(&underlying, input, msg_caller(), now_ns())
        })
    })
    .into()
}

/// Raises an underlying's multiplier immediately, or proposes lowering it through the timelock.
#[update(guard = "caller_is_risk_admin")]
#[must_use]
pub fn set_risk_multiplier(params: SetRiskMultiplierParams) -> RiskChangeResult {
    let SetRiskMultiplierParams { underlying, k } = params;
    fixed("k", &k)
        .and_then(|k| {
            RISK_STATE.with(|s| s.borrow_mut().set_k(&underlying, k, msg_caller(), now_ns()))
        })
        .into()
}

/// Raises the global multiplier immediately, or proposes lowering it through the timelock.
#[update(guard = "caller_is_risk_admin")]
#[must_use]
pub fn set_global_risk_multiplier(params: SetGlobalRiskMultiplierParams) -> RiskChangeResult {
    let SetGlobalRiskMultiplierParams { k } = params;
    fixed("global_k", &k)
        .and_then(|k| RISK_STATE.with(|s| s.borrow_mut().set_global_k(k, msg_caller(), now_ns())))
        .into()
}

/// Lengthens the timelock immediately, or proposes shortening it through the timelock itself.
#[update(guard = "caller_is_risk_admin")]
#[must_use]
pub fn set_timelock_duration(params: SetTimelockDurationParams) -> RiskChangeResult {
    let SetTimelockDurationParams { duration_ns } = params;
    RISK_STATE
        .with(|s| {
            s.borrow_mut()
                .set_timelock_duration(duration_ns, msg_caller(), now_ns())
        })
        .into()
}

// ----- Calendar -----

/// Adds scheduled events to an underlying's calendar. Returns their ids.
#[update(guard = "caller_is_risk_admin")]
#[must_use]
pub fn add_calendar_events(params: AddCalendarEventsParams) -> RiskIdsResult {
    let AddCalendarEventsParams { underlying, events } = params;
    let events: Result<Vec<NewEvent>, RiskError> = events
        .into_iter()
        .map(|e| {
            Ok(NewEvent {
                time_ns: e.time_ns,
                weight: fixed("weight", &e.weight)?,
                label: e.label,
            })
        })
        .collect();
    events
        .and_then(|events| {
            RISK_STATE.with(|s| {
                s.borrow_mut()
                    .add_calendar_events(&underlying, events, msg_caller(), now_ns())
            })
        })
        .into()
}

/// Removes one event from an underlying's calendar.
#[update(guard = "caller_is_risk_admin")]
#[must_use]
pub fn remove_calendar_event(params: RemoveCalendarEventParams) -> RiskUnitResult {
    let RemoveCalendarEventParams {
        underlying,
        event_id,
    } = params;
    RISK_STATE
        .with(|s| {
            s.borrow_mut()
                .remove_calendar_event(&underlying, event_id, msg_caller(), now_ns())
        })
        .into()
}

// ----- History backfill -----

/// Opens a history upload for an underlying, recording where the data comes from. Returns the
/// upload id.
#[update(guard = "caller_is_risk_admin")]
#[must_use]
pub fn begin_history_upload(params: BeginHistoryUploadParams) -> RiskIdResult {
    let BeginHistoryUploadParams {
        underlying,
        source,
        series_id,
    } = params;
    RISK_STATE
        .with(|s| {
            s.borrow_mut()
                .begin_upload(&underlying, source, series_id, msg_caller(), now_ns())
        })
        .into()
}

/// Appends a chunk of daily points to an open upload. Returns the upload's total point count.
#[update(guard = "caller_is_risk_admin")]
#[must_use]
pub fn append_history_chunk(params: AppendHistoryChunkParams) -> RiskCountResult {
    let AppendHistoryChunkParams { upload_id, points } = params;
    let points: Result<Vec<PricePoint>, RiskError> = points
        .iter()
        .map(|p| {
            Ok(PricePoint {
                date: p.date,
                price: fixed("price", &p.price)?,
            })
        })
        .collect();
    points
        .and_then(|points| RISK_STATE.with(|s| s.borrow_mut().append_chunk(upload_id, &points)))
        .into()
}

/// Verifies the declared SHA-256 of an upload and proposes its activation through the timelock.
/// Returns the pending action id.
#[update(guard = "caller_is_risk_admin")]
#[must_use]
pub fn seal_history_upload(params: SealHistoryUploadParams) -> RiskIdResult {
    let SealHistoryUploadParams {
        upload_id,
        sha256_hex,
    } = params;
    RISK_STATE
        .with(|s| {
            s.borrow_mut()
                .seal_upload(upload_id, &sha256_hex, msg_caller(), now_ns())
        })
        .into()
}

// ----- Timelock -----

/// Cancels a pending action. Cancelling an upload activation also cancels the upload.
#[update(guard = "caller_is_risk_admin_or_controller")]
#[must_use]
pub fn cancel_pending_action(action_id: u64) -> RiskUnitResult {
    RISK_STATE
        .with(|s| {
            s.borrow_mut()
                .cancel_action(action_id, msg_caller(), now_ns())
        })
        .into()
}

/// Executes a pending action once its timelock has expired. Anyone can execute: the action was
/// public for the whole delay, and it is re-validated against the current state.
#[update(guard = "caller_is_not_anonymous")]
#[must_use]
pub fn execute_pending_action(action_id: u64) -> RiskUnitResult {
    RISK_STATE
        .with(|s| {
            s.borrow_mut()
                .execute_action(action_id, &is_controller, msg_caller(), now_ns())
        })
        .into()
}

// ----- Queries -----

/// Risk administrators and global parameters.
#[query]
#[must_use]
pub fn get_risk_governance() -> RiskGovernanceView {
    RISK_STATE.with(|s| {
        let s = s.borrow();
        RiskGovernanceView {
            risk_admins: s.risk_admins.iter().copied().collect(),
            global_k: decimal(s.global.global_k),
            timelock_duration_ns: s.global.timelock_duration_ns,
        }
    })
}

/// Parameters of every configured underlying.
#[query]
#[must_use]
pub fn list_underlying_risk_params() -> Vec<UnderlyingRiskParamsView> {
    RISK_STATE.with(|s| {
        s.borrow()
            .underlyings
            .iter()
            .map(|(underlying, p)| UnderlyingRiskParamsView {
                underlying: underlying.clone(),
                lambda: decimal(p.lambda),
                weekend_weight: decimal(p.weekend_weight),
                floor_ratio: decimal(p.floor_ratio),
                mpor_days: p.mpor_days,
                k: decimal(p.k),
                oracle_asset_id: p.oracle_asset_id.clone(),
            })
            .collect()
    })
}

/// Calendar events of an underlying, in chronological order.
#[query]
#[must_use]
pub fn list_calendar_events(underlying: String) -> Vec<CalendarEventView> {
    let raw = underlying;
    let Ok(underlying) = canonical_underlying(&raw) else {
        return vec![];
    };
    RISK_STATE.with(|s| {
        s.borrow()
            .calendars
            .get(&underlying)
            .map(|events| {
                events
                    .iter()
                    .map(|e| CalendarEventView {
                        id: e.id,
                        time_ns: e.time_ns,
                        weight: decimal(e.weight),
                        label: e.label.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// Every action waiting behind the timelock.
#[query]
#[must_use]
pub fn list_pending_actions() -> Vec<PendingActionView> {
    RISK_STATE.with(|s| {
        s.borrow()
            .pending
            .values()
            .map(|p| PendingActionView {
                id: p.id,
                action: match &p.action {
                    TimelockedAction::LowerRiskMultiplier { underlying, k } => {
                        PendingActionKindView::LowerRiskMultiplier {
                            underlying: underlying.clone(),
                            k: decimal(*k),
                        }
                    }
                    TimelockedAction::LowerGlobalRiskMultiplier { k } => {
                        PendingActionKindView::LowerGlobalRiskMultiplier { k: decimal(*k) }
                    }
                    TimelockedAction::ActivateHistoryUpload { upload_id } => {
                        PendingActionKindView::ActivateHistoryUpload {
                            upload_id: *upload_id,
                        }
                    }
                    TimelockedAction::AddRiskAdmin { principal } => {
                        PendingActionKindView::AddRiskAdmin {
                            principal: *principal,
                        }
                    }
                    TimelockedAction::LowerTimelockDuration { duration_ns } => {
                        PendingActionKindView::LowerTimelockDuration {
                            duration_ns: *duration_ns,
                        }
                    }
                },
                proposed_by: p.proposed_by,
                proposed_at_ns: p.proposed_at_ns,
                executable_at_ns: p.executable_at_ns,
            })
            .collect()
    })
}

/// Metadata, provenance and hash of a history upload (not its points).
#[query]
#[must_use]
pub fn get_history_upload(upload_id: u64) -> Option<HistoryUploadView> {
    RISK_STATE.with(|s| {
        s.borrow()
            .uploads
            .get(&upload_id)
            .map(|u| HistoryUploadView {
                id: u.id,
                underlying: u.underlying.clone(),
                source: u.source.clone(),
                series_id: u.series_id.clone(),
                point_count: u.point_count,
                first_date: u.first_date,
                last_date: u.last_date,
                sha256: u.sha256.clone(),
                status: match u.status {
                    UploadStatus::Open => HistoryUploadStatusView::Open,
                    UploadStatus::Sealed { action_id } => {
                        HistoryUploadStatusView::Sealed { action_id }
                    }
                    UploadStatus::Activated { activated_at_ns } => {
                        HistoryUploadStatusView::Activated { activated_at_ns }
                    }
                    UploadStatus::Cancelled => HistoryUploadStatusView::Cancelled,
                },
                created_by: u.created_by,
                created_at_ns: u.created_at_ns,
            })
    })
}

/// A page of an underlying's active daily price history.
#[query]
#[must_use]
pub fn get_price_history(params: GetPriceHistoryParams) -> PriceHistoryResult {
    let GetPriceHistoryParams {
        underlying,
        from_date,
        limit,
    } = params;
    let limit = limit
        .unwrap_or(DEFAULT_HISTORY_LIMIT)
        .min(MAX_HISTORY_LIMIT) as usize;
    let from = from_date.unwrap_or(0);
    canonical_underlying(&underlying)
        .map(|underlying| {
            RISK_STATE.with(|s| {
                s.borrow()
                    .histories
                    .get(&underlying)
                    .map(|h| {
                        h.points
                            .iter()
                            .filter(|p| p.date >= from)
                            .take(limit)
                            .map(|p| PricePointView {
                                date: p.date,
                                price: decimal(p.price),
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            })
        })
        .into()
}

/// Current volatility of an underlying: EWMA, long-run and floored, with the volatility time over
/// its MPOR and its effective multiplier. Not yet an input to margin.
#[query]
#[must_use]
pub fn get_volatility(underlying: String) -> VolatilityResult {
    let raw = underlying;
    RISK_STATE
        .with(|s| s.borrow().volatility(&raw, now_ns()))
        .and_then(|reading| {
            Ok(VolatilityView {
                underlying: canonical_underlying(&raw)?,
                sigma_ewma: decimal(reading.estimate.sigma_ewma),
                sigma_long_run: decimal(reading.estimate.sigma_long_run),
                sigma_used: decimal(reading.estimate.sigma_used),
                returns: u32::try_from(reading.estimate.returns).unwrap_or(u32::MAX),
                last_date: reading.last_date,
                mpor_days: reading.mpor_days,
                mpor_volatility_time: decimal(reading.mpor_volatility_time),
                k_effective: decimal(reading.k_effective),
            })
        })
        .into()
}

/// A page of the public risk audit log, oldest first.
#[query]
#[must_use]
pub fn list_risk_audit(params: ListRiskAuditParams) -> Vec<RiskAuditEntry> {
    let ListRiskAuditParams { from_seq, limit } = params;
    let limit = limit.unwrap_or(DEFAULT_AUDIT_LIMIT).min(MAX_AUDIT_LIMIT) as usize;
    let from = usize::try_from(from_seq.unwrap_or(0)).unwrap_or(usize::MAX);
    RISK_STATE.with(|s| {
        s.borrow()
            .audit
            .iter()
            .skip(from)
            .take(limit)
            .cloned()
            .collect()
    })
}
