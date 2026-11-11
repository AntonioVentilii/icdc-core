//! The persisted risk state and every operation on it.
//!
//! Operations take the caller and the current time explicitly, so they can be tested without the
//! Internet Computer (`.agents/patterns.md`, section 3). The API layer only resolves authorization
//! and passes them in.

use core::mem;
use std::collections::{BTreeMap, BTreeSet};

use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use shared::types::{AssetId, DecimalValue};

use crate::{
    api::risk::errors::RiskError,
    risk::{
        calendar::{day_of, CalendarEvent, MAX_EVENTS_PER_UNDERLYING, MAX_LABEL_LEN, NS_PER_DAY},
        fixed::Fixed,
        history::{
            canonical_sha256, ChunkError, HistoryUpload, PriceHistory, PricePoint, UploadStatus,
            MAX_PENDING_UPLOADS, MAX_PROVENANCE_LEN,
        },
        params::{
            canonical_underlying, check_mpor_days, check_timelock_duration, GlobalRiskParams,
            UnderlyingRiskParams, EVENT_WEIGHT_BOUNDS, MULTIPLIER_BOUNDS,
        },
        timelock::{
            ActionKey, PendingAction, TimelockedAction, MAX_PENDING_ACTIONS,
            MAX_PENDING_CONTROLLER_ACTIONS,
        },
        volatility::{
            effective_multiplier, estimate, normalized_squared_returns, volatility_time,
            VolatilityEstimate, MIN_RETURNS,
        },
    },
};

/// Nanoseconds after which an oracle price is too old to be sampled into the history.
pub const SAMPLE_MAX_AGE_NS: u64 = 24 * 3_600 * 1_000_000_000;

/// An oracle price in USD and the time it was last updated, as read for sampling.
pub type OraclePrice = (DecimalValue, Option<u64>);

/// One entry of the public risk audit log.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RiskAuditEntry {
    pub seq: u64,
    pub time_ns: u64,
    pub caller: Principal,
    pub description: String,
}

/// Whether a requested change took effect or waits behind the timelock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskChangeOutcome {
    Applied,
    Pending {
        action_id: u64,
        executable_at_ns: u64,
    },
}

/// Who asks for a change of a multiplier. Raising is always safe, so controllers may raise as an
/// emergency brake; only risk administrators may propose lowering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiplierCaller {
    RiskAdmin,
    Controller,
}

/// A volatility reading for one underlying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolatilityReading {
    pub estimate: VolatilityEstimate,
    pub last_date: u32,
    pub mpor_days: u32,
    pub mpor_volatility_time: Fixed,
    pub k_effective: Fixed,
}

/// The parameters of an underlying that change without the timelock in phase 1 (everything except
/// `k`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnderlyingParamsInput {
    pub lambda: Fixed,
    pub weekend_weight: Fixed,
    pub floor_ratio: Fixed,
    pub mpor_days: u32,
    pub oracle_asset_id: Option<AssetId>,
}

/// Input of one new calendar event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewEvent {
    pub time_ns: u64,
    pub weight: Fixed,
    pub label: String,
}

/// Everything the risk engine persists. Stored as the optional `risk` field of `StableState`.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct RiskState {
    pub risk_admins: BTreeSet<Principal>,
    pub global: GlobalRiskParams,
    pub underlyings: BTreeMap<String, UnderlyingRiskParams>,
    pub calendars: BTreeMap<String, Vec<CalendarEvent>>,
    pub histories: BTreeMap<String, PriceHistory>,
    pub uploads: BTreeMap<u64, HistoryUpload>,
    pub pending: BTreeMap<u64, PendingAction>,
    pub audit: Vec<RiskAuditEntry>,
    pub next_event_id: u64,
    pub next_upload_id: u64,
    pub next_action_id: u64,
}

fn no_longer_valid(reason: impl Into<String>) -> RiskError {
    RiskError::ActionNoLongerValid(reason.into())
}

impl RiskState {
    fn audit(&mut self, caller: Principal, now_ns: u64, description: String) {
        let seq = u64::try_from(self.audit.len()).unwrap_or(u64::MAX);
        self.audit.push(RiskAuditEntry {
            seq,
            time_ns: now_ns,
            caller,
            description,
        });
    }

    fn known_underlying(&self, raw: &str) -> Result<String, RiskError> {
        let underlying = canonical_underlying(raw)?;
        if self.underlyings.contains_key(&underlying) {
            Ok(underlying)
        } else {
            Err(RiskError::UnknownUnderlying(underlying))
        }
    }

    fn pending_with_key(&self, key: &ActionKey) -> Vec<u64> {
        self.pending
            .values()
            .filter(|p| &p.action.key() == key)
            .map(|p| p.id)
            .collect()
    }

    /// Withdraws every pending action with `key`, recording why.
    fn withdraw_pending(&mut self, key: &ActionKey, reason: &str, caller: Principal, now_ns: u64) {
        for id in self.pending_with_key(key) {
            self.pending.remove(&id);
            self.audit(caller, now_ns, format!("withdrew action {id}: {reason}"));
        }
    }

    /// Proposes an action. A pending action with the same key is replaced. Controller actions and
    /// risk-administrator actions have separate caps, so one role cannot starve the other.
    fn propose(
        &mut self,
        action: TimelockedAction,
        caller: Principal,
        now_ns: u64,
    ) -> Result<PendingAction, RiskError> {
        let key = action.key();
        let controller_class = action.is_controller_action();
        let in_class = self
            .pending
            .values()
            .filter(|p| {
                p.action.is_controller_action() == controller_class && p.action.key() != key
            })
            .count();
        let cap = if controller_class {
            MAX_PENDING_CONTROLLER_ACTIONS
        } else {
            MAX_PENDING_ACTIONS
        };
        if in_class >= cap {
            return Err(RiskError::LimitReached("pending actions".to_owned()));
        }

        let id = self.next_action_id;
        self.next_action_id += 1;
        self.withdraw_pending(&key, &format!("replaced by action {id}"), caller, now_ns);
        let pending = PendingAction {
            id,
            action,
            proposed_by: caller,
            proposed_at_ns: now_ns,
            executable_at_ns: now_ns.saturating_add(self.global.timelock_duration_ns),
        };
        self.audit(
            caller,
            now_ns,
            format!("proposed action {id}: {:?}", pending.action),
        );
        self.pending.insert(id, pending.clone());
        Ok(pending)
    }

    fn pending_outcome(pending: &PendingAction) -> RiskChangeOutcome {
        RiskChangeOutcome::Pending {
            action_id: pending.id,
            executable_at_ns: pending.executable_at_ns,
        }
    }

    // ----- Roles -----

    /// Proposes granting the risk administrator role. `target_is_controller` must reflect the
    /// target's controller status: the two roles never sit on the same key.
    pub fn propose_risk_admin(
        &mut self,
        principal: Principal,
        target_is_controller: bool,
        caller: Principal,
        now_ns: u64,
    ) -> Result<u64, RiskError> {
        if principal == Principal::anonymous() {
            return Err(RiskError::InvalidPrincipal("anonymous".to_owned()));
        }
        if target_is_controller {
            return Err(RiskError::InvalidPrincipal(
                "a canister controller cannot be a risk administrator".to_owned(),
            ));
        }
        if self.risk_admins.contains(&principal) {
            return Err(RiskError::InvalidPrincipal(
                "already a risk administrator".to_owned(),
            ));
        }
        Ok(self
            .propose(TimelockedAction::AddRiskAdmin { principal }, caller, now_ns)?
            .id)
    }

    /// Removes a risk administrator immediately: removing power is always safe.
    pub fn remove_risk_admin(
        &mut self,
        principal: Principal,
        caller: Principal,
        now_ns: u64,
    ) -> Result<(), RiskError> {
        if !self.risk_admins.remove(&principal) {
            return Err(RiskError::InvalidPrincipal(
                "not a risk administrator".to_owned(),
            ));
        }
        self.audit(
            caller,
            now_ns,
            format!("removed risk administrator {principal}"),
        );
        Ok(())
    }

    // ----- Parameters -----

    /// Sets the non-multiplier parameters of an underlying, creating it with `k = 1` if new.
    pub fn set_underlying_params(
        &mut self,
        raw_underlying: &str,
        input: UnderlyingParamsInput,
        caller: Principal,
        now_ns: u64,
    ) -> Result<(), RiskError> {
        let underlying = canonical_underlying(raw_underlying)?;
        let k = self
            .underlyings
            .get(&underlying)
            .map_or(Fixed::ONE, |p| p.k);
        let params = UnderlyingRiskParams {
            lambda: input.lambda,
            weekend_weight: input.weekend_weight,
            floor_ratio: input.floor_ratio,
            mpor_days: input.mpor_days,
            k,
            oracle_asset_id: input.oracle_asset_id,
        };
        params.validate()?;
        self.audit(
            caller,
            now_ns,
            format!(
                "set {underlying} lambda={} weekend_weight={} floor_ratio={} mpor_days={} oracle_asset_id={:?}",
                params.lambda, params.weekend_weight, params.floor_ratio, params.mpor_days, params.oracle_asset_id
            ),
        );
        self.underlyings.insert(underlying, params);
        Ok(())
    }

    /// Raises `k` immediately (withdrawing any pending lowering of it) or, for a risk
    /// administrator, proposes lowering it.
    pub fn set_k(
        &mut self,
        raw_underlying: &str,
        k: Fixed,
        role: MultiplierCaller,
        caller: Principal,
        now_ns: u64,
    ) -> Result<RiskChangeOutcome, RiskError> {
        let underlying = self.known_underlying(raw_underlying)?;
        MULTIPLIER_BOUNDS.check("k", k)?;
        let current = self.underlyings[&underlying].k;
        if k >= current {
            if let Some(params) = self.underlyings.get_mut(&underlying) {
                params.k = k;
            }
            self.audit(
                caller,
                now_ns,
                format!("raised {underlying} k from {current} to {k}"),
            );
            self.withdraw_pending(
                &ActionKey::RiskMultiplier(underlying),
                "superseded by a raise",
                caller,
                now_ns,
            );
            return Ok(RiskChangeOutcome::Applied);
        }
        if role != MultiplierCaller::RiskAdmin {
            return Err(RiskError::Unauthorized(
                "only a risk administrator can propose lowering k".to_owned(),
            ));
        }
        let pending = self.propose(
            TimelockedAction::LowerRiskMultiplier {
                underlying,
                from: current,
                to: k,
            },
            caller,
            now_ns,
        )?;
        Ok(Self::pending_outcome(&pending))
    }

    /// Raises `global_k` immediately (withdrawing any pending lowering of it) or, for a risk
    /// administrator, proposes lowering it.
    pub fn set_global_k(
        &mut self,
        k: Fixed,
        role: MultiplierCaller,
        caller: Principal,
        now_ns: u64,
    ) -> Result<RiskChangeOutcome, RiskError> {
        MULTIPLIER_BOUNDS.check("global_k", k)?;
        let current = self.global.global_k;
        if k >= current {
            self.global.global_k = k;
            self.audit(
                caller,
                now_ns,
                format!("raised global_k from {current} to {k}"),
            );
            self.withdraw_pending(
                &ActionKey::GlobalRiskMultiplier,
                "superseded by a raise",
                caller,
                now_ns,
            );
            return Ok(RiskChangeOutcome::Applied);
        }
        if role != MultiplierCaller::RiskAdmin {
            return Err(RiskError::Unauthorized(
                "only a risk administrator can propose lowering global_k".to_owned(),
            ));
        }
        let pending = self.propose(
            TimelockedAction::LowerGlobalRiskMultiplier {
                from: current,
                to: k,
            },
            caller,
            now_ns,
        )?;
        Ok(Self::pending_outcome(&pending))
    }

    /// Lengthens the timelock immediately up to twice its current value; any shortening, and any
    /// lengthening beyond twice the current value, waits behind the timelock itself.
    pub fn set_timelock_duration(
        &mut self,
        duration_ns: u64,
        caller: Principal,
        now_ns: u64,
    ) -> Result<RiskChangeOutcome, RiskError> {
        check_timelock_duration(duration_ns)?;
        let current = self.global.timelock_duration_ns;
        if duration_ns >= current && duration_ns <= current.saturating_mul(2) {
            self.global.timelock_duration_ns = duration_ns;
            self.audit(
                caller,
                now_ns,
                format!("lengthened timelock from {current} to {duration_ns} ns"),
            );
            self.withdraw_pending(
                &ActionKey::TimelockDuration,
                "superseded by an immediate change",
                caller,
                now_ns,
            );
            return Ok(RiskChangeOutcome::Applied);
        }
        let pending = self.propose(
            TimelockedAction::ChangeTimelockDuration {
                from_ns: current,
                to_ns: duration_ns,
            },
            caller,
            now_ns,
        )?;
        Ok(Self::pending_outcome(&pending))
    }

    // ----- Calendar -----

    /// Adds events to an underlying's calendar. Events in the past, and exact duplicates (same
    /// time, weight and label) of an existing event or of another event in the batch, are rejected.
    /// Events that ended more than a day ago are pruned first, so the calendar never fills up with
    /// history.
    pub fn add_calendar_events(
        &mut self,
        raw_underlying: &str,
        events: Vec<NewEvent>,
        caller: Principal,
        now_ns: u64,
    ) -> Result<Vec<u64>, RiskError> {
        let underlying = self.known_underlying(raw_underlying)?;
        let existing = self
            .calendars
            .get(&underlying)
            .map_or(&[][..], Vec::as_slice);
        let invalid = |reason: String| RiskError::InvalidParameter {
            parameter: "events".to_owned(),
            reason,
        };
        for (index, event) in events.iter().enumerate() {
            EVENT_WEIGHT_BOUNDS.check("weight", event.weight)?;
            if event.label.chars().count() > MAX_LABEL_LEN {
                return Err(RiskError::InvalidParameter {
                    parameter: "label".to_owned(),
                    reason: format!("at most {MAX_LABEL_LEN} characters"),
                });
            }
            if event.time_ns < now_ns {
                return Err(invalid(format!(
                    "event {index} ({}) is in the past",
                    event.label
                )));
            }
            let same = |time_ns: u64, weight: Fixed, label: &str| {
                time_ns == event.time_ns && weight == event.weight && label == event.label
            };
            if existing.iter().any(|e| same(e.time_ns, e.weight, &e.label))
                || events[..index]
                    .iter()
                    .any(|e| same(e.time_ns, e.weight, &e.label))
            {
                return Err(invalid(format!(
                    "event {index} ({}) is a duplicate",
                    event.label
                )));
            }
        }

        let cutoff = now_ns.saturating_sub(NS_PER_DAY);
        let kept = existing.iter().filter(|e| e.time_ns >= cutoff).count();
        let pruned = existing.len() - kept;
        if kept + events.len() > MAX_EVENTS_PER_UNDERLYING {
            return Err(RiskError::LimitReached(format!(
                "calendar events for {underlying}"
            )));
        }

        let mut ids = Vec::with_capacity(events.len());
        let mut added = Vec::with_capacity(events.len());
        for event in events {
            let id = self.next_event_id;
            self.next_event_id += 1;
            ids.push(id);
            added.push(CalendarEvent {
                id,
                time_ns: event.time_ns,
                weight: event.weight,
                label: event.label,
            });
        }
        self.audit(
            caller,
            now_ns,
            format!("added calendar events {ids:?} to {underlying}, pruned {pruned} past events"),
        );
        let calendar = self.calendars.entry(underlying).or_default();
        calendar.retain(|e| e.time_ns >= cutoff);
        calendar.extend(added);
        calendar.sort_by_key(|e| (e.time_ns, e.id));
        Ok(ids)
    }

    pub fn remove_calendar_event(
        &mut self,
        raw_underlying: &str,
        event_id: u64,
        caller: Principal,
        now_ns: u64,
    ) -> Result<(), RiskError> {
        let underlying = self.known_underlying(raw_underlying)?;
        let calendar = self
            .calendars
            .get_mut(&underlying)
            .ok_or(RiskError::CalendarEventNotFound(event_id))?;
        let before = calendar.len();
        calendar.retain(|e| e.id != event_id);
        if calendar.len() == before {
            return Err(RiskError::CalendarEventNotFound(event_id));
        }
        self.audit(
            caller,
            now_ns,
            format!("removed calendar event {event_id} from {underlying}"),
        );
        Ok(())
    }

    // ----- History backfill -----

    pub fn begin_upload(
        &mut self,
        raw_underlying: &str,
        source: String,
        series_id: String,
        caller: Principal,
        now_ns: u64,
    ) -> Result<u64, RiskError> {
        let underlying = self.known_underlying(raw_underlying)?;
        for (name, value) in [("source", &source), ("series_id", &series_id)] {
            if value.trim().is_empty() || value.chars().count() > MAX_PROVENANCE_LEN {
                return Err(RiskError::InvalidParameter {
                    parameter: name.to_owned(),
                    reason: format!("must be 1 to {MAX_PROVENANCE_LEN} characters"),
                });
            }
        }
        let in_flight = self
            .uploads
            .values()
            .filter(|u| matches!(u.status, UploadStatus::Open | UploadStatus::Sealed { .. }))
            .count();
        if in_flight >= MAX_PENDING_UPLOADS {
            return Err(RiskError::LimitReached("open or sealed uploads".to_owned()));
        }

        let id = self.next_upload_id;
        self.next_upload_id += 1;
        self.audit(
            caller,
            now_ns,
            format!("opened upload {id} for {underlying} from {source} ({series_id})"),
        );
        self.uploads.insert(
            id,
            HistoryUpload {
                id,
                underlying,
                source,
                series_id,
                points: vec![],
                point_count: 0,
                first_date: None,
                last_date: None,
                sha256: None,
                status: UploadStatus::Open,
                created_by: caller,
                created_at_ns: now_ns,
            },
        );
        Ok(id)
    }

    pub fn append_chunk(
        &mut self,
        upload_id: u64,
        points: &[PricePoint],
        now_ns: u64,
    ) -> Result<u32, RiskError> {
        let upload = self
            .uploads
            .get_mut(&upload_id)
            .ok_or(RiskError::UploadNotFound(upload_id))?;
        if upload.status != UploadStatus::Open {
            return Err(RiskError::UploadNotOpen(upload_id));
        }
        upload.append(points, day_of(now_ns)).map_err(|e| {
            RiskError::InvalidHistoryChunk(match e {
                ChunkError::Empty => "empty chunk".to_owned(),
                ChunkError::TooLarge => "chunk too large".to_owned(),
                ChunkError::UploadFull => "upload full".to_owned(),
                ChunkError::NotIncreasing { date } => format!("date {date} does not increase"),
                ChunkError::NonPositivePrice { date } => {
                    format!("non-positive price on date {date}")
                }
                ChunkError::FutureDate { date } => format!("date {date} is in the future"),
            })
        })?;
        Ok(upload.point_count)
    }

    /// Verifies the declared hash and proposes the activation through the timelock.
    pub fn seal_upload(
        &mut self,
        upload_id: u64,
        declared_sha256: &str,
        caller: Principal,
        now_ns: u64,
    ) -> Result<u64, RiskError> {
        let upload = self
            .uploads
            .get(&upload_id)
            .ok_or(RiskError::UploadNotFound(upload_id))?;
        if upload.status != UploadStatus::Open {
            return Err(RiskError::UploadNotOpen(upload_id));
        }
        if upload.points.is_empty() {
            return Err(RiskError::InvalidHistoryChunk(
                "upload has no points".to_owned(),
            ));
        }
        let computed = canonical_sha256(&upload.points);
        if declared_sha256.trim().to_ascii_lowercase() != computed {
            return Err(RiskError::HashMismatch);
        }

        let pending = self.propose(
            TimelockedAction::ActivateHistoryUpload { upload_id },
            caller,
            now_ns,
        )?;
        if let Some(upload) = self.uploads.get_mut(&upload_id) {
            upload.sha256 = Some(computed);
            upload.status = UploadStatus::Sealed {
                action_id: pending.id,
            };
        }
        Ok(pending.id)
    }

    /// Abandons an open upload, or a sealed one together with its pending activation. Its points
    /// are dropped and it frees its slot.
    pub fn abandon_upload(
        &mut self,
        upload_id: u64,
        caller: Principal,
        now_ns: u64,
    ) -> Result<(), RiskError> {
        let upload = self
            .uploads
            .get_mut(&upload_id)
            .ok_or(RiskError::UploadNotFound(upload_id))?;
        let sealed_by = match upload.status {
            UploadStatus::Open => None,
            UploadStatus::Sealed { action_id } => Some(action_id),
            UploadStatus::Activated { .. } | UploadStatus::Cancelled => {
                return Err(RiskError::UploadNotOpen(upload_id));
            }
        };
        upload.status = UploadStatus::Cancelled;
        upload.points = vec![];
        if let Some(action_id) = sealed_by {
            self.pending.remove(&action_id);
        }
        self.audit(caller, now_ns, format!("abandoned upload {upload_id}"));
        Ok(())
    }

    /// Proposes removing the history points of an underlying dated `from_date..=to_date`, for
    /// example a bad print. Execution checks that the range still holds exactly the points it held
    /// when proposed.
    pub fn propose_history_removal(
        &mut self,
        raw_underlying: &str,
        from_date: u32,
        to_date: u32,
        caller: Principal,
        now_ns: u64,
    ) -> Result<u64, RiskError> {
        let underlying = self.known_underlying(raw_underlying)?;
        if from_date > to_date {
            return Err(RiskError::InvalidParameter {
                parameter: "from_date".to_owned(),
                reason: "must not be after to_date".to_owned(),
            });
        }
        let range = self
            .histories
            .get(&underlying)
            .map_or(&[][..], |h| h.range(from_date, to_date));
        if range.is_empty() {
            return Err(RiskError::InvalidParameter {
                parameter: "from_date".to_owned(),
                reason: "no history points in this range".to_owned(),
            });
        }
        let expected_sha256 = canonical_sha256(range);
        Ok(self
            .propose(
                TimelockedAction::RemoveHistoryPoints {
                    underlying,
                    from_date,
                    to_date,
                    expected_sha256,
                },
                caller,
                now_ns,
            )?
            .id)
    }

    // ----- Timelock -----

    pub fn cancel_action(
        &mut self,
        action_id: u64,
        caller: Principal,
        now_ns: u64,
    ) -> Result<(), RiskError> {
        let pending = self
            .pending
            .remove(&action_id)
            .ok_or(RiskError::PendingActionNotFound(action_id))?;
        if let TimelockedAction::ActivateHistoryUpload { upload_id } = pending.action {
            if let Some(upload) = self.uploads.get_mut(&upload_id) {
                upload.status = UploadStatus::Cancelled;
                upload.points = vec![];
            }
        }
        self.audit(caller, now_ns, format!("cancelled action {action_id}"));
        Ok(())
    }

    /// Executes a pending action once its delay has passed. The action is re-validated against the
    /// current state: in particular, an action that changes a value is rejected if that value is no
    /// longer the one it was proposed against.
    pub fn execute_action(
        &mut self,
        action_id: u64,
        is_controller: &dyn Fn(&Principal) -> bool,
        caller: Principal,
        now_ns: u64,
    ) -> Result<(), RiskError> {
        let pending = self
            .pending
            .get(&action_id)
            .cloned()
            .ok_or(RiskError::PendingActionNotFound(action_id))?;
        if now_ns < pending.executable_at_ns {
            return Err(RiskError::TimelockNotExpired {
                executable_at_ns: pending.executable_at_ns,
            });
        }

        match &pending.action {
            TimelockedAction::LowerRiskMultiplier {
                underlying,
                from,
                to,
            } => {
                MULTIPLIER_BOUNDS.check("k", *to)?;
                let params = self
                    .underlyings
                    .get_mut(underlying)
                    .ok_or_else(|| no_longer_valid(format!("unknown underlying {underlying}")))?;
                if params.k != *from {
                    return Err(no_longer_valid(format!(
                        "{underlying} k is {} now, not {from}",
                        params.k
                    )));
                }
                params.k = *to;
            }
            TimelockedAction::LowerGlobalRiskMultiplier { from, to } => {
                MULTIPLIER_BOUNDS.check("global_k", *to)?;
                if self.global.global_k != *from {
                    return Err(no_longer_valid(format!(
                        "global_k is {} now, not {from}",
                        self.global.global_k
                    )));
                }
                self.global.global_k = *to;
            }
            TimelockedAction::ChangeTimelockDuration { from_ns, to_ns } => {
                check_timelock_duration(*to_ns)?;
                if self.global.timelock_duration_ns != *from_ns {
                    return Err(no_longer_valid(format!(
                        "the timelock is {} ns now, not {from_ns}",
                        self.global.timelock_duration_ns
                    )));
                }
                self.global.timelock_duration_ns = *to_ns;
            }
            TimelockedAction::AddRiskAdmin { principal } => {
                if is_controller(principal) {
                    return Err(no_longer_valid(
                        "the principal has become a canister controller",
                    ));
                }
                if !self.risk_admins.insert(*principal) {
                    return Err(no_longer_valid("already a risk administrator"));
                }
            }
            TimelockedAction::ActivateHistoryUpload { upload_id } => {
                let upload = self
                    .uploads
                    .get_mut(upload_id)
                    .ok_or(RiskError::UploadNotFound(*upload_id))?;
                if upload.status != (UploadStatus::Sealed { action_id }) {
                    return Err(no_longer_valid(format!(
                        "upload {upload_id} is not sealed by this action"
                    )));
                }
                let points = mem::take(&mut upload.points);
                upload.status = UploadStatus::Activated {
                    activated_at_ns: now_ns,
                };
                let underlying = upload.underlying.clone();
                self.histories.entry(underlying).or_default().merge(&points);
            }
            TimelockedAction::RemoveHistoryPoints {
                underlying,
                from_date,
                to_date,
                expected_sha256,
            } => {
                let history = self
                    .histories
                    .get_mut(underlying)
                    .ok_or_else(|| no_longer_valid(format!("no history for {underlying}")))?;
                if &canonical_sha256(history.range(*from_date, *to_date)) != expected_sha256 {
                    return Err(no_longer_valid(
                        "the points in the range changed after the removal was proposed",
                    ));
                }
                history.remove_range(*from_date, *to_date);
            }
        }

        self.pending.remove(&action_id);
        self.audit(caller, now_ns, format!("executed action {action_id}"));
        Ok(())
    }

    // ----- Sampling -----

    /// Appends today's point for every underlying whose oracle asset has a price updated within
    /// [`SAMPLE_MAX_AGE_NS`]. `price_of` returns the asset's USD price and its last update time.
    /// Returns the number of points appended. Not called in phase 1 (see `risk::sampler`).
    pub fn sample_daily(
        &mut self,
        now_ns: u64,
        price_of: &dyn Fn(&AssetId) -> Option<OraclePrice>,
    ) -> usize {
        let today = day_of(now_ns);
        let mut appended = 0;
        let sources: Vec<(String, AssetId)> = self
            .underlyings
            .iter()
            .filter_map(|(u, p)| p.oracle_asset_id.clone().map(|a| (u.clone(), a)))
            .collect();
        for (underlying, asset_id) in sources {
            let Some((price, updated_at)) = price_of(&asset_id) else {
                continue;
            };
            let fresh = updated_at.is_some_and(|t| now_ns.saturating_sub(t) <= SAMPLE_MAX_AGE_NS);
            let Ok(price) = Fixed::from_decimal(&price) else {
                continue;
            };
            if !fresh || price <= Fixed::ZERO {
                continue;
            }
            let history = self.histories.entry(underlying).or_default();
            if history.append_if_new(PricePoint { date: today, price }) {
                appended += 1;
            }
        }
        appended
    }

    // ----- Readings -----

    pub fn volatility(
        &self,
        raw_underlying: &str,
        now_ns: u64,
    ) -> Result<VolatilityReading, RiskError> {
        let underlying = self.known_underlying(raw_underlying)?;
        let params = &self.underlyings[&underlying];
        let points = self
            .histories
            .get(&underlying)
            .map_or(&[][..], |h| h.points.as_slice());
        let returns = normalized_squared_returns(points, params.weekend_weight)?;
        let insufficient = || RiskError::InsufficientHistory {
            returns: u32::try_from(returns.len()).unwrap_or(u32::MAX),
            required: u32::try_from(MIN_RETURNS).unwrap_or(u32::MAX),
        };
        let estimate =
            estimate(&returns, params.lambda, params.floor_ratio)?.ok_or_else(insufficient)?;
        let events = self
            .calendars
            .get(&underlying)
            .map_or(&[][..], Vec::as_slice);
        check_mpor_days(params.mpor_days)?;
        Ok(VolatilityReading {
            estimate,
            last_date: points.last().map_or(0, |p| p.date),
            mpor_days: params.mpor_days,
            mpor_volatility_time: volatility_time(
                now_ns,
                params.mpor_days,
                params.weekend_weight,
                events,
            )?,
            k_effective: effective_multiplier(params.k, self.global.global_k)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use candid::Principal;
    use shared::types::{AssetId, DecimalValue};

    use super::{
        MultiplierCaller, NewEvent, RiskChangeOutcome, RiskState, UnderlyingParamsInput,
        SAMPLE_MAX_AGE_NS,
    };
    use crate::{
        api::risk::errors::RiskError,
        risk::{
            calendar::NS_PER_DAY,
            fixed::Fixed,
            history::{canonical_sha256, PricePoint, UploadStatus},
            params::{DEFAULT_TIMELOCK_DURATION_NS, TIMELOCK_DURATION_MAX_NS},
            timelock::{TimelockedAction, MAX_PENDING_ACTIONS},
        },
    };

    const NOW: u64 = 20_766 * NS_PER_DAY;
    const ADMIN: MultiplierCaller = MultiplierCaller::RiskAdmin;
    const AFTER: u64 = NOW + DEFAULT_TIMELOCK_DURATION_NS;

    fn admin() -> Principal {
        Principal::from_slice(&[1])
    }

    fn not_controller(_: &Principal) -> bool {
        false
    }

    fn input(oracle_asset_id: Option<AssetId>) -> UnderlyingParamsInput {
        UnderlyingParamsInput {
            lambda: Fixed::from_raw(940_000_000_000),
            weekend_weight: Fixed::ONE,
            floor_ratio: Fixed::from_raw(750_000_000_000),
            mpor_days: 3,
            oracle_asset_id,
        }
    }

    fn state_with_btc() -> RiskState {
        let mut state = RiskState::default();
        state
            .set_underlying_params("btc", input(None), admin(), NOW)
            .unwrap();
        state
    }

    fn pending_id(outcome: RiskChangeOutcome) -> u64 {
        let RiskChangeOutcome::Pending { action_id, .. } = outcome else {
            panic!("expected a pending change, got {outcome:?}");
        };
        action_id
    }

    #[test]
    fn raising_k_applies_and_lowering_waits_for_the_timelock() {
        let mut state = state_with_btc();
        assert_eq!(
            state
                .set_k("BTC", Fixed::from_int(2), ADMIN, admin(), NOW)
                .unwrap(),
            RiskChangeOutcome::Applied
        );
        assert_eq!(state.underlyings["BTC"].k, Fixed::from_int(2));

        let outcome = state.set_k("BTC", Fixed::ONE, ADMIN, admin(), NOW).unwrap();
        let RiskChangeOutcome::Pending {
            action_id,
            executable_at_ns,
        } = outcome
        else {
            panic!("lowering k must be pending");
        };
        assert_eq!(executable_at_ns, AFTER);
        assert_eq!(
            state.underlyings["BTC"].k,
            Fixed::from_int(2),
            "unchanged until executed"
        );

        assert_eq!(
            state.execute_action(action_id, &not_controller, admin(), AFTER - 1),
            Err(RiskError::TimelockNotExpired {
                executable_at_ns: AFTER
            })
        );
        state
            .execute_action(action_id, &not_controller, admin(), AFTER)
            .unwrap();
        assert_eq!(state.underlyings["BTC"].k, Fixed::ONE);
        assert!(state.pending.is_empty());

        assert!(matches!(
            state.set_k("BTC", Fixed::from_int(4), ADMIN, admin(), NOW),
            Err(RiskError::OutOfBounds { .. })
        ));
        assert!(matches!(
            state.set_k("ETH", Fixed::from_int(2), ADMIN, admin(), NOW),
            Err(RiskError::UnknownUnderlying(_))
        ));
    }

    #[test]
    fn an_emergency_raise_withdraws_a_pending_lowering() {
        // The review's scenario: lowering proposed, a crisis hits, k is raised; the old lowering
        // must not fire afterwards.
        let mut state = state_with_btc();
        state
            .set_k("BTC", Fixed::from_int(2), ADMIN, admin(), NOW)
            .unwrap();
        let lowering = pending_id(
            state
                .set_k(
                    "BTC",
                    Fixed::from_raw(1_500_000_000_000),
                    ADMIN,
                    admin(),
                    NOW,
                )
                .unwrap(),
        );
        state
            .set_k("BTC", Fixed::from_int(3), ADMIN, admin(), NOW + 1)
            .unwrap();
        assert_eq!(
            state.execute_action(lowering, &not_controller, admin(), AFTER),
            Err(RiskError::PendingActionNotFound(lowering))
        );
        assert_eq!(state.underlyings["BTC"].k, Fixed::from_int(3));
    }

    #[test]
    fn execution_rejects_an_action_whose_starting_value_changed() {
        let mut state = state_with_btc();
        state
            .set_k("BTC", Fixed::from_int(2), ADMIN, admin(), NOW)
            .unwrap();
        let lowering = pending_id(state.set_k("BTC", Fixed::ONE, ADMIN, admin(), NOW).unwrap());
        // The value moves by any other route: the action no longer describes the current state.
        if let Some(params) = state.underlyings.get_mut("BTC") {
            params.k = Fixed::from_raw(2_500_000_000_000);
        }
        assert!(matches!(
            state.execute_action(lowering, &not_controller, admin(), AFTER),
            Err(RiskError::ActionNoLongerValid(_))
        ));
        assert_eq!(
            state.underlyings["BTC"].k,
            Fixed::from_raw(2_500_000_000_000)
        );
    }

    #[test]
    fn one_pending_action_per_key() {
        let mut state = state_with_btc();
        state
            .set_k("BTC", Fixed::from_int(3), ADMIN, admin(), NOW)
            .unwrap();
        let first = pending_id(
            state
                .set_k("BTC", Fixed::from_int(2), ADMIN, admin(), NOW)
                .unwrap(),
        );
        let second = pending_id(state.set_k("BTC", Fixed::ONE, ADMIN, admin(), NOW).unwrap());
        assert_eq!(state.pending.len(), 1);
        assert!(!state.pending.contains_key(&first));
        assert!(matches!(
            state.pending[&second].action,
            TimelockedAction::LowerRiskMultiplier { to, .. } if to == Fixed::ONE
        ));
    }

    #[test]
    fn controllers_may_raise_multipliers_but_not_lower_them() {
        let mut state = state_with_btc();
        let controller = MultiplierCaller::Controller;
        assert_eq!(
            state
                .set_k("BTC", Fixed::from_int(2), controller, admin(), NOW)
                .unwrap(),
            RiskChangeOutcome::Applied
        );
        assert!(matches!(
            state.set_k("BTC", Fixed::ONE, controller, admin(), NOW),
            Err(RiskError::Unauthorized(_))
        ));
        assert_eq!(
            state
                .set_global_k(Fixed::from_int(2), controller, admin(), NOW)
                .unwrap(),
            RiskChangeOutcome::Applied
        );
        assert!(matches!(
            state.set_global_k(Fixed::ONE, controller, admin(), NOW),
            Err(RiskError::Unauthorized(_))
        ));
        assert!(state.pending.is_empty());
    }

    #[test]
    fn global_k_follows_the_multiplier_rule() {
        let mut state = RiskState::default();
        assert_eq!(
            state
                .set_global_k(Fixed::from_int(2), ADMIN, admin(), NOW)
                .unwrap(),
            RiskChangeOutcome::Applied
        );
        let lowering = pending_id(state.set_global_k(Fixed::ONE, ADMIN, admin(), NOW).unwrap());
        state
            .execute_action(lowering, &not_controller, admin(), AFTER)
            .unwrap();
        assert_eq!(state.global.global_k, Fixed::ONE);
    }

    #[test]
    fn the_timelock_doubles_at_most_immediately_and_is_bounded() {
        let mut state = RiskState::default();
        let current = DEFAULT_TIMELOCK_DURATION_NS;
        // Beyond twice the current value (48 h to 7 days): timelocked.
        let longer = pending_id(
            state
                .set_timelock_duration(TIMELOCK_DURATION_MAX_NS, admin(), NOW)
                .unwrap(),
        );
        // Shortening: timelocked, and it replaces the pending lengthening (same key).
        let shorter = pending_id(
            state
                .set_timelock_duration(current / 2, admin(), NOW)
                .unwrap(),
        );
        assert!(!state.pending.contains_key(&longer));
        assert!(state.pending.contains_key(&shorter));
        // Above the hard maximum: rejected.
        assert!(matches!(
            state.set_timelock_duration(TIMELOCK_DURATION_MAX_NS + 1, admin(), NOW),
            Err(RiskError::InvalidParameter { .. })
        ));
        // Up to twice the current value: immediate, and it withdraws the pending shortening.
        assert_eq!(
            state
                .set_timelock_duration(current * 2, admin(), NOW)
                .unwrap(),
            RiskChangeOutcome::Applied
        );
        assert_eq!(state.global.timelock_duration_ns, current * 2);
        assert!(state.pending.is_empty());
        // The withdrawn shortening can no longer run.
        assert_eq!(
            state.execute_action(shorter, &not_controller, admin(), u64::MAX),
            Err(RiskError::PendingActionNotFound(shorter))
        );
    }

    #[test]
    fn risk_administrators_cannot_crowd_out_controller_actions() {
        let mut state = RiskState::default();
        for i in 0..MAX_PENDING_ACTIONS {
            let underlying = format!("U{i}");
            state
                .set_underlying_params(&underlying, input(None), admin(), NOW)
                .unwrap();
            state
                .set_k(&underlying, Fixed::from_int(2), ADMIN, admin(), NOW)
                .unwrap();
            pending_id(
                state
                    .set_k(&underlying, Fixed::ONE, ADMIN, admin(), NOW)
                    .unwrap(),
            );
        }
        state
            .set_underlying_params("FULL", input(None), admin(), NOW)
            .unwrap();
        state
            .set_k("FULL", Fixed::from_int(2), ADMIN, admin(), NOW)
            .unwrap();
        assert!(matches!(
            state.set_k("FULL", Fixed::ONE, ADMIN, admin(), NOW),
            Err(RiskError::LimitReached(_))
        ));
        // A controller can still propose a risk administrator.
        state
            .propose_risk_admin(Principal::from_slice(&[9]), false, admin(), NOW)
            .unwrap();
    }

    #[test]
    fn risk_admins_are_timelocked_and_never_controllers() {
        let mut state = RiskState::default();
        let candidate = Principal::from_slice(&[2]);
        assert!(matches!(
            state.propose_risk_admin(candidate, true, admin(), NOW),
            Err(RiskError::InvalidPrincipal(_))
        ));
        assert!(matches!(
            state.propose_risk_admin(Principal::anonymous(), false, admin(), NOW),
            Err(RiskError::InvalidPrincipal(_))
        ));

        let action_id = state
            .propose_risk_admin(candidate, false, admin(), NOW)
            .unwrap();
        assert!(!state.risk_admins.contains(&candidate));
        // A principal that became a controller meanwhile is refused at execution.
        assert!(matches!(
            state.execute_action(action_id, &|_| true, admin(), AFTER),
            Err(RiskError::ActionNoLongerValid(_))
        ));
        state
            .execute_action(action_id, &not_controller, admin(), AFTER)
            .unwrap();
        assert!(state.risk_admins.contains(&candidate));

        state.remove_risk_admin(candidate, admin(), AFTER).unwrap();
        assert!(!state.risk_admins.contains(&candidate));
    }

    #[test]
    fn cancelled_actions_never_execute() {
        let mut state = state_with_btc();
        state
            .set_k("BTC", Fixed::from_int(2), ADMIN, admin(), NOW)
            .unwrap();
        let action_id = pending_id(state.set_k("BTC", Fixed::ONE, ADMIN, admin(), NOW).unwrap());
        state.cancel_action(action_id, admin(), NOW).unwrap();
        assert_eq!(
            state.execute_action(action_id, &not_controller, admin(), u64::MAX),
            Err(RiskError::PendingActionNotFound(action_id))
        );
        assert_eq!(state.underlyings["BTC"].k, Fixed::from_int(2));
    }

    fn points(count: u32) -> Vec<PricePoint> {
        // Alternating +1% and -1% moves on consecutive days.
        let mut price = Fixed::from_int(100);
        (0..count)
            .map(|i| {
                if i > 0 {
                    let factor = if i % 2 == 0 {
                        Fixed::from_raw(1_010_000_000_000)
                    } else {
                        Fixed::from_raw(990_000_000_000)
                    };
                    price = price.checked_mul(factor).unwrap();
                }
                PricePoint {
                    date: 20_000 + i,
                    price,
                }
            })
            .collect()
    }

    fn activate(state: &mut RiskState, data: &[PricePoint]) {
        let upload_id = state
            .begin_upload(
                "BTC",
                "coinbase".to_owned(),
                "BTC-USD".to_owned(),
                admin(),
                NOW,
            )
            .unwrap();
        state.append_chunk(upload_id, data, NOW).unwrap();
        let action_id = state
            .seal_upload(upload_id, &canonical_sha256(data), admin(), NOW)
            .unwrap();
        state
            .execute_action(action_id, &not_controller, admin(), AFTER)
            .unwrap();
    }

    #[test]
    fn backfill_is_hash_checked_and_timelocked() {
        let mut state = state_with_btc();
        let upload_id = state
            .begin_upload(
                "BTC",
                "coinbase".to_owned(),
                "BTC-USD".to_owned(),
                admin(),
                NOW,
            )
            .unwrap();
        let data = points(60);
        state.append_chunk(upload_id, &data[..30], NOW).unwrap();
        assert_eq!(state.append_chunk(upload_id, &data[30..], NOW).unwrap(), 60);

        assert_eq!(
            state.seal_upload(upload_id, &"0".repeat(64), admin(), NOW),
            Err(RiskError::HashMismatch)
        );
        let hash = canonical_sha256(&data);
        let action_id = state
            .seal_upload(upload_id, &hash.to_ascii_uppercase(), admin(), NOW)
            .unwrap();
        assert_eq!(
            state.uploads[&upload_id].sha256.as_deref(),
            Some(hash.as_str())
        );
        assert!(matches!(
            state.append_chunk(upload_id, &points(1), NOW),
            Err(RiskError::UploadNotOpen(_))
        ));
        assert!(
            !state.histories.contains_key("BTC"),
            "nothing merged before the timelock"
        );

        state
            .execute_action(action_id, &not_controller, admin(), AFTER)
            .unwrap();
        assert_eq!(state.histories["BTC"].points, data);
        assert!(matches!(
            state.uploads[&upload_id].status,
            UploadStatus::Activated { .. }
        ));
        assert!(state.uploads[&upload_id].points.is_empty());

        let reading = state.volatility("BTC", NOW).unwrap();
        assert_eq!(reading.estimate.returns, 59);
        assert!(reading.estimate.is_provisional);
        assert_eq!(reading.estimate.sigma_long_run, None);
        assert_eq!(reading.last_date, 20_059);
        assert_eq!(reading.k_effective, Fixed::ONE);
        // Constant 1% daily moves: volatility is close to 1%.
        let sigma = reading.estimate.sigma_ewma.raw();
        assert!(
            (9_900_000_000..=10_100_000_000).contains(&sigma),
            "sigma {sigma}"
        );
    }

    #[test]
    fn uploads_reject_future_dates() {
        let mut state = state_with_btc();
        let upload_id = state
            .begin_upload("BTC", "test".to_owned(), "x".to_owned(), admin(), NOW)
            .unwrap();
        let tomorrow = [PricePoint {
            date: 20_767,
            price: Fixed::ONE,
        }];
        assert!(matches!(
            state.append_chunk(upload_id, &tomorrow, NOW),
            Err(RiskError::InvalidHistoryChunk(_))
        ));
    }

    #[test]
    fn cancelling_an_activation_cancels_the_upload() {
        let mut state = state_with_btc();
        let upload_id = state
            .begin_upload(
                "BTC",
                "coinbase".to_owned(),
                "BTC-USD".to_owned(),
                admin(),
                NOW,
            )
            .unwrap();
        let data = points(5);
        state.append_chunk(upload_id, &data, NOW).unwrap();
        let action_id = state
            .seal_upload(upload_id, &canonical_sha256(&data), admin(), NOW)
            .unwrap();
        state.cancel_action(action_id, admin(), NOW).unwrap();
        assert_eq!(state.uploads[&upload_id].status, UploadStatus::Cancelled);
        assert!(state.uploads[&upload_id].points.is_empty());
    }

    #[test]
    fn open_and_sealed_uploads_can_be_abandoned() {
        let mut state = state_with_btc();
        let data = points(5);
        let open = state
            .begin_upload("BTC", "test".to_owned(), "a".to_owned(), admin(), NOW)
            .unwrap();
        state.append_chunk(open, &data, NOW).unwrap();
        state.abandon_upload(open, admin(), NOW).unwrap();
        assert_eq!(state.uploads[&open].status, UploadStatus::Cancelled);
        assert!(state.uploads[&open].points.is_empty());

        let sealed = state
            .begin_upload("BTC", "test".to_owned(), "b".to_owned(), admin(), NOW)
            .unwrap();
        state.append_chunk(sealed, &data, NOW).unwrap();
        let action_id = state
            .seal_upload(sealed, &canonical_sha256(&data), admin(), NOW)
            .unwrap();
        state.abandon_upload(sealed, admin(), NOW).unwrap();
        assert!(!state.pending.contains_key(&action_id));
        assert_eq!(
            state.abandon_upload(sealed, admin(), NOW),
            Err(RiskError::UploadNotOpen(sealed))
        );
    }

    #[test]
    fn a_bad_range_can_be_removed_through_the_timelock() {
        let mut state = state_with_btc();
        activate(&mut state, &points(10));
        let removal = state
            .propose_history_removal("BTC", 20_003, 20_004, admin(), NOW)
            .unwrap();
        assert_eq!(
            state.histories["BTC"].points.len(),
            10,
            "nothing before execution"
        );
        state
            .execute_action(removal, &not_controller, admin(), AFTER)
            .unwrap();
        let history = &state.histories["BTC"];
        assert_eq!(history.points.len(), 8);
        assert!(history.range(20_003, 20_004).is_empty());

        assert!(matches!(
            state.propose_history_removal("BTC", 20_003, 20_004, admin(), NOW),
            Err(RiskError::InvalidParameter { .. })
        ));
    }

    #[test]
    fn a_removal_is_rejected_if_the_range_changed_meanwhile() {
        let mut state = state_with_btc();
        activate(&mut state, &points(10));
        let removal = state
            .propose_history_removal("BTC", 20_003, 20_004, admin(), NOW)
            .unwrap();
        // A corrected value for 20_004 is merged before the removal executes.
        state.histories.get_mut("BTC").unwrap().merge(&[PricePoint {
            date: 20_004,
            price: Fixed::from_int(123),
        }]);
        assert!(matches!(
            state.execute_action(removal, &not_controller, admin(), AFTER),
            Err(RiskError::ActionNoLongerValid(_))
        ));
        assert_eq!(state.histories["BTC"].points.len(), 10);
    }

    #[test]
    fn insufficient_history_is_reported() {
        let state = state_with_btc();
        assert_eq!(
            state.volatility("BTC", NOW),
            Err(RiskError::InsufficientHistory {
                returns: 0,
                required: 21
            })
        );
    }

    #[test]
    fn sampling_appends_fresh_prices_once_per_day() {
        let mut state = RiskState::default();
        let asset = AssetId::from("ckBTC".to_owned());
        state
            .set_underlying_params("BTC", input(Some(asset.clone())), admin(), NOW)
            .unwrap();
        let price = DecimalValue::new(10_500_000, 2);

        let fresh = |a: &AssetId| (a == &asset).then(|| (price.clone(), Some(NOW - 1)));
        assert_eq!(state.sample_daily(NOW, &fresh), 1);
        assert_eq!(state.sample_daily(NOW + 1, &fresh), 0, "one point per day");
        assert_eq!(
            state.histories["BTC"].points,
            vec![PricePoint {
                date: 20_766,
                price: Fixed::from_int(105_000)
            }]
        );

        let stale = |_: &AssetId| Some((price.clone(), Some(NOW - SAMPLE_MAX_AGE_NS - 1)));
        assert_eq!(
            state.sample_daily(NOW + NS_PER_DAY, &stale),
            0,
            "stale prices are skipped"
        );
        let unknown_age = |_: &AssetId| Some((price.clone(), None));
        assert_eq!(state.sample_daily(NOW + NS_PER_DAY, &unknown_age), 0);
    }

    fn event(time_ns: u64, weight: i64, label: &str) -> NewEvent {
        NewEvent {
            time_ns,
            weight: Fixed::from_int(weight),
            label: label.to_owned(),
        }
    }

    #[test]
    fn calendar_events_are_bounded_and_removable() {
        let mut state = state_with_btc();
        let ids = state
            .add_calendar_events(
                "BTC",
                vec![event(NOW + NS_PER_DAY, 3, "FOMC")],
                admin(),
                NOW,
            )
            .unwrap();
        assert!(matches!(
            state.add_calendar_events(
                "BTC",
                vec![event(NOW + NS_PER_DAY, 11, "FOMC")],
                admin(),
                NOW
            ),
            Err(RiskError::OutOfBounds { .. })
        ));
        // The event raises volatility time over the MPOR.
        let reading_vt =
            super::volatility_time(NOW, 3, Fixed::ONE, &state.calendars["BTC"]).unwrap();
        assert!(reading_vt > Fixed::from_int(3));
        state
            .remove_calendar_event("BTC", ids[0], admin(), NOW)
            .unwrap();
        assert_eq!(
            state.remove_calendar_event("BTC", ids[0], admin(), NOW),
            Err(RiskError::CalendarEventNotFound(ids[0]))
        );
    }

    #[test]
    fn calendar_rejects_past_and_duplicate_events_and_prunes_old_ones() {
        let mut state = state_with_btc();
        assert!(matches!(
            state.add_calendar_events("BTC", vec![event(NOW - 1, 3, "past")], admin(), NOW),
            Err(RiskError::InvalidParameter { .. })
        ));
        let fomc = event(NOW + NS_PER_DAY, 3, "FOMC");
        assert!(matches!(
            state.add_calendar_events("BTC", vec![fomc.clone(), fomc.clone()], admin(), NOW),
            Err(RiskError::InvalidParameter { .. })
        ));
        state
            .add_calendar_events("BTC", vec![fomc.clone()], admin(), NOW)
            .unwrap();
        assert!(matches!(
            state.add_calendar_events("BTC", vec![fomc], admin(), NOW),
            Err(RiskError::InvalidParameter { .. })
        ));

        // Three days later the FOMC event ended more than a day ago: adding anything prunes it.
        let later = NOW + 3 * NS_PER_DAY;
        state
            .add_calendar_events(
                "BTC",
                vec![event(later + NS_PER_DAY, 2, "CPI")],
                admin(),
                later,
            )
            .unwrap();
        let labels: Vec<&str> = state.calendars["BTC"]
            .iter()
            .map(|e| e.label.as_str())
            .collect();
        assert_eq!(labels, vec!["CPI"]);
    }

    #[test]
    fn every_change_is_audited_with_its_content() {
        let mut state = state_with_btc();
        state
            .set_k("BTC", Fixed::from_int(2), ADMIN, admin(), NOW)
            .unwrap();
        let lowering = pending_id(
            state
                .set_k("BTC", Fixed::ONE, ADMIN, admin(), NOW + 1)
                .unwrap(),
        );
        state
            .execute_action(lowering, &not_controller, admin(), AFTER + 1)
            .unwrap();

        let entries: Vec<(u64, u64, &str)> = state
            .audit
            .iter()
            .map(|e| (e.seq, e.time_ns, e.description.as_str()))
            .collect();
        assert_eq!(entries.len(), 4);
        assert!(state.audit.iter().all(|e| e.caller == admin()));
        assert_eq!(
            entries.iter().map(|e| e.0).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert!(entries[0].2.starts_with("set BTC lambda="));
        assert_eq!(
            entries[1],
            (1, NOW, "raised BTC k from 1.000000000000 to 2.000000000000")
        );
        assert!(entries[2]
            .2
            .starts_with(&format!("proposed action {lowering}: LowerRiskMultiplier")));
        assert_eq!(entries[2].1, NOW + 1);
        assert_eq!(
            entries[3],
            (3, AFTER + 1, &*format!("executed action {lowering}"))
        );
    }
}
