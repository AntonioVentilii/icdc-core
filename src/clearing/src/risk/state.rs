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
        calendar::{day_of, CalendarEvent, MAX_EVENTS_PER_UNDERLYING, MAX_LABEL_LEN},
        fixed::Fixed,
        history::{
            canonical_sha256, ChunkError, HistoryUpload, PriceHistory, PricePoint, UploadStatus,
            MAX_PENDING_UPLOADS, MAX_PROVENANCE_LEN,
        },
        params::{
            canonical_underlying, check_mpor_days, check_timelock_duration, GlobalRiskParams,
            UnderlyingRiskParams, EVENT_WEIGHT_BOUNDS, MULTIPLIER_BOUNDS,
        },
        timelock::{PendingAction, TimelockedAction, MAX_PENDING_ACTIONS},
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
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum RiskChangeOutcome {
    Applied,
    Pending {
        action_id: u64,
        executable_at_ns: u64,
    },
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

    fn propose(
        &mut self,
        action: TimelockedAction,
        caller: Principal,
        now_ns: u64,
    ) -> Result<PendingAction, RiskError> {
        if self.pending.len() >= MAX_PENDING_ACTIONS {
            return Err(RiskError::LimitReached("pending actions".to_owned()));
        }
        let id = self.next_action_id;
        self.next_action_id += 1;
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

    /// Raises `k` immediately or proposes lowering it.
    pub fn set_k(
        &mut self,
        raw_underlying: &str,
        k: Fixed,
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
            return Ok(RiskChangeOutcome::Applied);
        }
        let pending = self.propose(
            TimelockedAction::LowerRiskMultiplier { underlying, k },
            caller,
            now_ns,
        )?;
        Ok(Self::pending_outcome(&pending))
    }

    /// Raises `global_k` immediately or proposes lowering it.
    pub fn set_global_k(
        &mut self,
        k: Fixed,
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
            return Ok(RiskChangeOutcome::Applied);
        }
        let pending = self.propose(
            TimelockedAction::LowerGlobalRiskMultiplier { k },
            caller,
            now_ns,
        )?;
        Ok(Self::pending_outcome(&pending))
    }

    /// Lengthens the timelock immediately or proposes shortening it.
    pub fn set_timelock_duration(
        &mut self,
        duration_ns: u64,
        caller: Principal,
        now_ns: u64,
    ) -> Result<RiskChangeOutcome, RiskError> {
        check_timelock_duration(duration_ns)?;
        let current = self.global.timelock_duration_ns;
        if duration_ns >= current {
            self.global.timelock_duration_ns = duration_ns;
            self.audit(
                caller,
                now_ns,
                format!("lengthened timelock from {current} to {duration_ns} ns"),
            );
            return Ok(RiskChangeOutcome::Applied);
        }
        let pending = self.propose(
            TimelockedAction::LowerTimelockDuration { duration_ns },
            caller,
            now_ns,
        )?;
        Ok(Self::pending_outcome(&pending))
    }

    // ----- Calendar -----

    pub fn add_calendar_events(
        &mut self,
        raw_underlying: &str,
        events: Vec<NewEvent>,
        caller: Principal,
        now_ns: u64,
    ) -> Result<Vec<u64>, RiskError> {
        let underlying = self.known_underlying(raw_underlying)?;
        for event in &events {
            EVENT_WEIGHT_BOUNDS.check("weight", event.weight)?;
            if event.label.chars().count() > MAX_LABEL_LEN {
                return Err(RiskError::InvalidParameter {
                    parameter: "label".to_owned(),
                    reason: format!("at most {MAX_LABEL_LEN} characters"),
                });
            }
        }
        let existing = self.calendars.get(&underlying).map_or(0, Vec::len);
        if existing + events.len() > MAX_EVENTS_PER_UNDERLYING {
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
            format!("added calendar events {ids:?} to {underlying}"),
        );
        let calendar = self.calendars.entry(underlying).or_default();
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
    ) -> Result<u32, RiskError> {
        let upload = self
            .uploads
            .get_mut(&upload_id)
            .ok_or(RiskError::UploadNotFound(upload_id))?;
        if upload.status != UploadStatus::Open {
            return Err(RiskError::UploadNotOpen(upload_id));
        }
        upload.append(points).map_err(|e| {
            RiskError::InvalidHistoryChunk(match e {
                ChunkError::Empty => "empty chunk".to_owned(),
                ChunkError::TooLarge => "chunk too large".to_owned(),
                ChunkError::UploadFull => "upload full".to_owned(),
                ChunkError::NotIncreasing { date } => format!("date {date} does not increase"),
                ChunkError::NonPositivePrice { date } => {
                    format!("non-positive price on date {date}")
                }
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
            return Err(RiskError::HashMismatch { computed });
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

    /// Executes a pending action once its delay has passed, re-validating it against the current
    /// state.
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
            TimelockedAction::LowerRiskMultiplier { underlying, k } => {
                MULTIPLIER_BOUNDS.check("k", *k)?;
                let params = self.underlyings.get_mut(underlying).ok_or_else(|| {
                    RiskError::ActionNoLongerValid(format!("unknown underlying {underlying}"))
                })?;
                params.k = *k;
            }
            TimelockedAction::LowerGlobalRiskMultiplier { k } => {
                MULTIPLIER_BOUNDS.check("global_k", *k)?;
                self.global.global_k = *k;
            }
            TimelockedAction::LowerTimelockDuration { duration_ns } => {
                check_timelock_duration(*duration_ns)?;
                self.global.timelock_duration_ns = *duration_ns;
            }
            TimelockedAction::AddRiskAdmin { principal } => {
                if is_controller(principal) {
                    return Err(RiskError::ActionNoLongerValid(
                        "the principal has become a canister controller".to_owned(),
                    ));
                }
                self.risk_admins.insert(*principal);
            }
            TimelockedAction::ActivateHistoryUpload { upload_id } => {
                let upload = self
                    .uploads
                    .get_mut(upload_id)
                    .ok_or(RiskError::UploadNotFound(*upload_id))?;
                if upload.status != (UploadStatus::Sealed { action_id }) {
                    return Err(RiskError::ActionNoLongerValid(format!(
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
        }

        self.pending.remove(&action_id);
        self.audit(caller, now_ns, format!("executed action {action_id}"));
        Ok(())
    }

    // ----- Sampling -----

    /// Appends today's point for every underlying whose oracle asset has a price updated within
    /// [`SAMPLE_MAX_AGE_NS`]. `price_of` returns the asset's USD price and its last update time.
    /// Returns the number of points appended.
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

    use super::{NewEvent, RiskChangeOutcome, RiskState, UnderlyingParamsInput, SAMPLE_MAX_AGE_NS};
    use crate::{
        api::risk::errors::RiskError,
        risk::{
            calendar::NS_PER_DAY,
            fixed::Fixed,
            history::{canonical_sha256, PricePoint, UploadStatus},
            params::DEFAULT_TIMELOCK_DURATION_NS,
            timelock::TimelockedAction,
        },
    };

    const NOW: u64 = 20_766 * NS_PER_DAY;

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

    #[test]
    fn raising_k_applies_and_lowering_waits_for_the_timelock() {
        let mut state = state_with_btc();
        assert_eq!(
            state
                .set_k("BTC", Fixed::from_int(2), admin(), NOW)
                .unwrap(),
            RiskChangeOutcome::Applied
        );
        assert_eq!(state.underlyings["BTC"].k, Fixed::from_int(2));

        let outcome = state.set_k("BTC", Fixed::ONE, admin(), NOW).unwrap();
        let RiskChangeOutcome::Pending {
            action_id,
            executable_at_ns,
        } = outcome
        else {
            panic!("lowering k must be pending");
        };
        assert_eq!(executable_at_ns, NOW + DEFAULT_TIMELOCK_DURATION_NS);
        assert_eq!(
            state.underlyings["BTC"].k,
            Fixed::from_int(2),
            "unchanged until executed"
        );

        assert_eq!(
            state.execute_action(action_id, &not_controller, admin(), executable_at_ns - 1),
            Err(RiskError::TimelockNotExpired { executable_at_ns })
        );
        state
            .execute_action(action_id, &not_controller, admin(), executable_at_ns)
            .unwrap();
        assert_eq!(state.underlyings["BTC"].k, Fixed::ONE);
        assert!(state.pending.is_empty());

        assert!(matches!(
            state.set_k("BTC", Fixed::from_int(4), admin(), NOW),
            Err(RiskError::OutOfBounds { .. })
        ));
        assert!(matches!(
            state.set_k("ETH", Fixed::from_int(2), admin(), NOW),
            Err(RiskError::UnknownUnderlying(_))
        ));
    }

    #[test]
    fn global_k_and_timelock_follow_the_same_rule() {
        let mut state = RiskState::default();
        assert_eq!(
            state
                .set_global_k(Fixed::from_int(2), admin(), NOW)
                .unwrap(),
            RiskChangeOutcome::Applied
        );
        assert!(matches!(
            state.set_global_k(Fixed::ONE, admin(), NOW).unwrap(),
            RiskChangeOutcome::Pending { .. }
        ));
        let longer = DEFAULT_TIMELOCK_DURATION_NS * 2;
        assert_eq!(
            state.set_timelock_duration(longer, admin(), NOW).unwrap(),
            RiskChangeOutcome::Applied
        );
        assert!(matches!(
            state
                .set_timelock_duration(DEFAULT_TIMELOCK_DURATION_NS, admin(), NOW)
                .unwrap(),
            RiskChangeOutcome::Pending { .. }
        ));
    }

    #[test]
    fn cancelled_actions_never_execute() {
        let mut state = state_with_btc();
        state
            .set_k("BTC", Fixed::from_int(2), admin(), NOW)
            .unwrap();
        let RiskChangeOutcome::Pending { action_id, .. } =
            state.set_k("BTC", Fixed::ONE, admin(), NOW).unwrap()
        else {
            panic!("expected pending");
        };
        state.cancel_action(action_id, admin(), NOW).unwrap();
        assert_eq!(
            state.execute_action(action_id, &not_controller, admin(), u64::MAX),
            Err(RiskError::PendingActionNotFound(action_id))
        );
        assert_eq!(state.underlyings["BTC"].k, Fixed::from_int(2));
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
        let at = NOW + DEFAULT_TIMELOCK_DURATION_NS;
        // A principal that became a controller meanwhile is refused at execution.
        assert!(matches!(
            state.execute_action(action_id, &|_| true, admin(), at),
            Err(RiskError::ActionNoLongerValid(_))
        ));
        state
            .execute_action(action_id, &not_controller, admin(), at)
            .unwrap();
        assert!(state.risk_admins.contains(&candidate));

        state.remove_risk_admin(candidate, admin(), at).unwrap();
        assert!(!state.risk_admins.contains(&candidate));
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
        state.append_chunk(upload_id, &data[..30]).unwrap();
        assert_eq!(state.append_chunk(upload_id, &data[30..]).unwrap(), 60);

        assert!(matches!(
            state.seal_upload(upload_id, &"0".repeat(64), admin(), NOW),
            Err(RiskError::HashMismatch { .. })
        ));
        let hash = canonical_sha256(&data);
        let action_id = state
            .seal_upload(upload_id, &hash.to_ascii_uppercase(), admin(), NOW)
            .unwrap();
        assert_eq!(
            state.uploads[&upload_id].sha256.as_deref(),
            Some(hash.as_str())
        );
        assert!(matches!(
            state.append_chunk(upload_id, &points(1)),
            Err(RiskError::UploadNotOpen(_))
        ));
        assert!(
            !state.histories.contains_key("BTC"),
            "nothing merged before the timelock"
        );

        state
            .execute_action(
                action_id,
                &not_controller,
                admin(),
                NOW + DEFAULT_TIMELOCK_DURATION_NS,
            )
            .unwrap();
        assert_eq!(state.histories["BTC"].points, data);
        assert!(matches!(
            state.uploads[&upload_id].status,
            UploadStatus::Activated { .. }
        ));
        assert!(state.uploads[&upload_id].points.is_empty());

        let reading = state.volatility("BTC", NOW).unwrap();
        assert_eq!(reading.estimate.returns, 59);
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
        state.append_chunk(upload_id, &data).unwrap();
        let action_id = state
            .seal_upload(upload_id, &canonical_sha256(&data), admin(), NOW)
            .unwrap();
        state.cancel_action(action_id, admin(), NOW).unwrap();
        assert_eq!(state.uploads[&upload_id].status, UploadStatus::Cancelled);
        assert!(state.uploads[&upload_id].points.is_empty());
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

    #[test]
    fn calendar_events_are_bounded_and_removable() {
        let mut state = state_with_btc();
        let event = |weight: Fixed| NewEvent {
            time_ns: NOW + NS_PER_DAY,
            weight,
            label: "FOMC".to_owned(),
        };
        let ids = state
            .add_calendar_events("BTC", vec![event(Fixed::from_int(3))], admin(), NOW)
            .unwrap();
        assert!(matches!(
            state.add_calendar_events("BTC", vec![event(Fixed::from_int(11))], admin(), NOW),
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
    fn every_change_is_audited() {
        let mut state = state_with_btc();
        state
            .set_k("BTC", Fixed::from_int(2), admin(), NOW)
            .unwrap();
        assert_eq!(state.audit.len(), 2);
        assert!(state.audit.iter().all(|e| e.caller == admin()));
        assert_eq!(
            state.audit.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert!(matches!(
            state.pending.values().next().map(|p| &p.action),
            None | Some(TimelockedAction::LowerRiskMultiplier { .. })
        ));
    }
}
