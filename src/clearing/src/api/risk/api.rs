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
        GetHistoryUploadPointsParams, GetPriceHistoryParams, HistoryUploadParams,
        ListRiskAuditParams, PendingActionParams, ProposeHistoryRemovalParams,
        RemoveCalendarEventParams, RiskAdminParams, SealHistoryUploadParams,
        SetGlobalRiskMultiplierParams, SetRiskMultiplierParams, SetTimelockDurationParams,
        SetUnderlyingRiskParamsParams, UnderlyingParams,
    },
    results::{
        CalendarEventView, CalendarEventsResult, HistoryUploadStatusView, HistoryUploadView,
        PendingActionKindView, PendingActionView, PendingActionsResult, PriceHistoryResult,
        PricePointView, RiskAuditEntryView, RiskChangeResult, RiskChangeView, RiskCountResult,
        RiskGovernanceResult, RiskGovernanceView, RiskIdResult, RiskIdsResult, RiskUnitResult,
        UnderlyingRiskParamsResult, UnderlyingRiskParamsView, VolatilityResult, VolatilityView,
    },
};
use crate::{
    guards::{caller_is_controller, caller_is_risk_admin, caller_is_risk_admin_or_controller},
    memory::RISK_STATE,
    risk::{
        fixed::Fixed,
        history::{PricePoint, UploadStatus},
        params::canonical_underlying,
        state::{MultiplierCaller, NewEvent, RiskChangeOutcome, UnderlyingParamsInput},
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

/// Risk figures are never negative. A negative value would mean a bug, so it is reported as an
/// error rather than shown as a plausible number.
fn decimal(value: Fixed) -> Result<DecimalValue, RiskError> {
    value
        .to_decimal()
        .map_err(|e| RiskError::ArithmeticError(format!("{value}: {e}")))
}

fn points_view(points: &[PricePoint], limit: usize) -> Result<Vec<PricePointView>, RiskError> {
    points
        .iter()
        .take(limit)
        .map(|p| {
            Ok(PricePointView {
                date: p.date,
                price: decimal(p.price)?,
            })
        })
        .collect()
}

fn page_limit(limit: Option<u32>) -> usize {
    limit
        .unwrap_or(DEFAULT_HISTORY_LIMIT)
        .min(MAX_HISTORY_LIMIT) as usize
}

fn change_view(outcome: RiskChangeOutcome) -> RiskChangeView {
    match outcome {
        RiskChangeOutcome::Applied => RiskChangeView::Applied,
        RiskChangeOutcome::Pending {
            action_id,
            executable_at_ns,
        } => RiskChangeView::Pending {
            action_id,
            executable_at_ns,
        },
    }
}

/// The role of a caller that passed `caller_is_risk_admin_or_controller`.
fn multiplier_caller() -> MultiplierCaller {
    if caller_is_risk_admin().is_ok() {
        MultiplierCaller::RiskAdmin
    } else {
        MultiplierCaller::Controller
    }
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

/// Raises an underlying's multiplier immediately, or proposes lowering it through the timelock. A
/// raise withdraws any pending lowering of the same multiplier. Controllers may raise (an emergency
/// brake) but only risk administrators may propose lowering.
#[update(guard = "caller_is_risk_admin_or_controller")]
#[must_use]
pub fn set_risk_multiplier(params: SetRiskMultiplierParams) -> RiskChangeResult {
    let SetRiskMultiplierParams { underlying, k } = params;
    let role = multiplier_caller();
    fixed("k", &k)
        .and_then(|k| {
            RISK_STATE.with(|s| {
                s.borrow_mut()
                    .set_k(&underlying, k, role, msg_caller(), now_ns())
            })
        })
        .map(change_view)
        .into()
}

/// Raises the global multiplier immediately, or proposes lowering it through the timelock. Same
/// roles as `set_risk_multiplier`.
#[update(guard = "caller_is_risk_admin_or_controller")]
#[must_use]
pub fn set_global_risk_multiplier(params: SetGlobalRiskMultiplierParams) -> RiskChangeResult {
    let SetGlobalRiskMultiplierParams { k } = params;
    let role = multiplier_caller();
    fixed("global_k", &k)
        .and_then(|k| {
            RISK_STATE.with(|s| s.borrow_mut().set_global_k(k, role, msg_caller(), now_ns()))
        })
        .map(change_view)
        .into()
}

/// Lengthens the timelock immediately up to twice its current value, or proposes any other change
/// (shortening, or lengthening beyond twice) through the timelock itself. Bounded to 24 hours to 7
/// days.
#[update(guard = "caller_is_risk_admin")]
#[must_use]
pub fn set_timelock_duration(params: SetTimelockDurationParams) -> RiskChangeResult {
    let SetTimelockDurationParams { duration_ns } = params;
    RISK_STATE
        .with(|s| {
            s.borrow_mut()
                .set_timelock_duration(duration_ns, msg_caller(), now_ns())
        })
        .map(change_view)
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
        .and_then(|points| {
            RISK_STATE.with(|s| s.borrow_mut().append_chunk(upload_id, &points, now_ns()))
        })
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

/// Abandons an open or sealed upload (and its pending activation), freeing its slot.
#[update(guard = "caller_is_risk_admin_or_controller")]
#[must_use]
pub fn abandon_history_upload(params: HistoryUploadParams) -> RiskUnitResult {
    let HistoryUploadParams { upload_id } = params;
    RISK_STATE
        .with(|s| {
            s.borrow_mut()
                .abandon_upload(upload_id, msg_caller(), now_ns())
        })
        .into()
}

/// Proposes removing the active history points of an underlying in `from_date..=to_date`, for
/// example a bad print. Executes after the timelock, only if the range still holds the same points.
/// Returns the pending action id.
#[update(guard = "caller_is_risk_admin")]
#[must_use]
pub fn propose_history_removal(params: ProposeHistoryRemovalParams) -> RiskIdResult {
    let ProposeHistoryRemovalParams {
        underlying,
        from_date,
        to_date,
    } = params;
    RISK_STATE
        .with(|s| {
            s.borrow_mut().propose_history_removal(
                &underlying,
                from_date,
                to_date,
                msg_caller(),
                now_ns(),
            )
        })
        .into()
}

// ----- Timelock -----

/// Cancels a pending action. Cancelling an upload activation also cancels the upload.
#[update(guard = "caller_is_risk_admin_or_controller")]
#[must_use]
pub fn cancel_pending_action(params: PendingActionParams) -> RiskUnitResult {
    let PendingActionParams { action_id } = params;
    RISK_STATE
        .with(|s| {
            s.borrow_mut()
                .cancel_action(action_id, msg_caller(), now_ns())
        })
        .into()
}

/// Executes a pending action once its timelock has expired. Restricted to risk administrators and
/// controllers, so execution happens when the people who can still cancel are watching. The action
/// is re-validated against the current state.
#[update(guard = "caller_is_risk_admin_or_controller")]
#[must_use]
pub fn execute_pending_action(params: PendingActionParams) -> RiskUnitResult {
    let PendingActionParams { action_id } = params;
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
pub fn get_risk_governance() -> RiskGovernanceResult {
    RISK_STATE
        .with(|s| {
            let s = s.borrow();
            Ok(RiskGovernanceView {
                risk_admins: s.risk_admins.iter().copied().collect(),
                global_k: decimal(s.global.global_k)?,
                timelock_duration_ns: s.global.timelock_duration_ns,
            })
        })
        .into()
}

/// Parameters of every configured underlying.
#[query]
#[must_use]
pub fn list_underlying_risk_params() -> UnderlyingRiskParamsResult {
    RISK_STATE
        .with(|s| {
            s.borrow()
                .underlyings
                .iter()
                .map(|(underlying, p)| {
                    Ok(UnderlyingRiskParamsView {
                        underlying: underlying.clone(),
                        lambda: decimal(p.lambda)?,
                        weekend_weight: decimal(p.weekend_weight)?,
                        floor_ratio: decimal(p.floor_ratio)?,
                        mpor_days: p.mpor_days,
                        k: decimal(p.k)?,
                        oracle_asset_id: p.oracle_asset_id.clone(),
                    })
                })
                .collect::<Result<Vec<_>, RiskError>>()
        })
        .into()
}

/// Calendar events of an underlying, in chronological order.
#[query]
#[must_use]
pub fn list_calendar_events(params: UnderlyingParams) -> CalendarEventsResult {
    let UnderlyingParams { underlying } = params;
    canonical_underlying(&underlying)
        .and_then(|underlying| {
            RISK_STATE.with(|s| {
                s.borrow()
                    .calendars
                    .get(&underlying)
                    .map_or(&[][..], Vec::as_slice)
                    .iter()
                    .map(|e| {
                        Ok(CalendarEventView {
                            id: e.id,
                            time_ns: e.time_ns,
                            weight: decimal(e.weight)?,
                            label: e.label.clone(),
                        })
                    })
                    .collect()
            })
        })
        .into()
}

fn pending_kind_view(action: &TimelockedAction) -> Result<PendingActionKindView, RiskError> {
    Ok(match action {
        TimelockedAction::LowerRiskMultiplier {
            underlying,
            from,
            to,
        } => PendingActionKindView::LowerRiskMultiplier {
            underlying: underlying.clone(),
            from: decimal(*from)?,
            to: decimal(*to)?,
        },
        TimelockedAction::LowerGlobalRiskMultiplier { from, to } => {
            PendingActionKindView::LowerGlobalRiskMultiplier {
                from: decimal(*from)?,
                to: decimal(*to)?,
            }
        }
        TimelockedAction::ChangeTimelockDuration { from_ns, to_ns } => {
            PendingActionKindView::ChangeTimelockDuration {
                from_ns: *from_ns,
                to_ns: *to_ns,
            }
        }
        TimelockedAction::ActivateHistoryUpload { upload_id } => {
            PendingActionKindView::ActivateHistoryUpload {
                upload_id: *upload_id,
            }
        }
        TimelockedAction::RemoveHistoryPoints {
            underlying,
            from_date,
            to_date,
            expected_sha256,
        } => PendingActionKindView::RemoveHistoryPoints {
            underlying: underlying.clone(),
            from_date: *from_date,
            to_date: *to_date,
            expected_sha256: expected_sha256.clone(),
        },
        TimelockedAction::AddRiskAdmin { principal } => PendingActionKindView::AddRiskAdmin {
            principal: *principal,
        },
    })
}

/// Every action waiting behind the timelock.
#[query]
#[must_use]
pub fn list_pending_actions() -> PendingActionsResult {
    RISK_STATE
        .with(|s| {
            s.borrow()
                .pending
                .values()
                .map(|p| {
                    Ok(PendingActionView {
                        id: p.id,
                        action: pending_kind_view(&p.action)?,
                        proposed_by: p.proposed_by,
                        proposed_at_ns: p.proposed_at_ns,
                        executable_at_ns: p.executable_at_ns,
                    })
                })
                .collect::<Result<Vec<_>, RiskError>>()
        })
        .into()
}

/// Metadata, provenance and hash of a history upload. Its points are paged by
/// `get_history_upload_points`.
#[query]
#[must_use]
pub fn get_history_upload(params: HistoryUploadParams) -> Option<HistoryUploadView> {
    let HistoryUploadParams { upload_id } = params;
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

/// A page of the points of an upload that is still open or sealed, so anyone can check the data
/// against the public source during the timelock. Activated and cancelled uploads hold no points.
#[query]
#[must_use]
pub fn get_history_upload_points(params: GetHistoryUploadPointsParams) -> PriceHistoryResult {
    let GetHistoryUploadPointsParams {
        upload_id,
        from_date,
        limit,
    } = params;
    RISK_STATE
        .with(|s| {
            let s = s.borrow();
            let upload = s
                .uploads
                .get(&upload_id)
                .ok_or(RiskError::UploadNotFound(upload_id))?;
            let from = from_date.unwrap_or(0);
            let start = upload.points.partition_point(|p| p.date < from);
            points_view(&upload.points[start..], page_limit(limit))
        })
        .into()
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
    canonical_underlying(&underlying)
        .and_then(|underlying| {
            RISK_STATE.with(|s| {
                let s = s.borrow();
                let points = s
                    .histories
                    .get(&underlying)
                    .map_or(&[][..], |h| h.range(from_date.unwrap_or(0), u32::MAX));
                points_view(points, page_limit(limit))
            })
        })
        .into()
}

/// Current volatility of an underlying: EWMA, and once 250 returns exist long-run and floored, with
/// the volatility time over its MPOR and its effective multiplier. Not yet an input to margin.
#[query]
#[must_use]
pub fn get_volatility(params: UnderlyingParams) -> VolatilityResult {
    let UnderlyingParams { underlying: raw } = params;
    RISK_STATE
        .with(|s| s.borrow().volatility(&raw, now_ns()))
        .and_then(|reading| {
            let estimate = reading.estimate;
            Ok(VolatilityView {
                underlying: canonical_underlying(&raw)?,
                sigma_ewma: decimal(estimate.sigma_ewma)?,
                sigma_long_run: estimate.sigma_long_run.map(decimal).transpose()?,
                sigma_used: estimate.sigma_used.map(decimal).transpose()?,
                is_provisional: estimate.is_provisional,
                returns: u32::try_from(estimate.returns).unwrap_or(u32::MAX),
                last_date: reading.last_date,
                mpor_days: reading.mpor_days,
                mpor_volatility_time: decimal(reading.mpor_volatility_time)?,
                k_effective: decimal(reading.k_effective)?,
            })
        })
        .into()
}

/// A page of the public risk audit log, oldest first.
#[query]
#[must_use]
pub fn list_risk_audit(params: ListRiskAuditParams) -> Vec<RiskAuditEntryView> {
    let ListRiskAuditParams { from_seq, limit } = params;
    let limit = limit.unwrap_or(DEFAULT_AUDIT_LIMIT).min(MAX_AUDIT_LIMIT) as usize;
    let from = usize::try_from(from_seq.unwrap_or(0)).unwrap_or(usize::MAX);
    RISK_STATE.with(|s| {
        s.borrow()
            .audit
            .iter()
            .skip(from)
            .take(limit)
            .map(|e| RiskAuditEntryView {
                seq: e.seq,
                time_ns: e.time_ns,
                caller: e.caller,
                description: e.description.clone(),
            })
            .collect()
    })
}
