use core::time::Duration;
use std::fs;

use candid::{encode_args, CandidType, Deserialize, Principal};
use clearing::{
    api::{
        admin::params::UpdateAssetMetricsParams,
        risk::{
            errors::RiskError,
            params::{
                AppendHistoryChunkParams, BeginHistoryUploadParams, GetHistoryUploadPointsParams,
                GetPriceHistoryParams, ListRiskAuditParams, PendingActionParams, PricePointInput,
                RiskAdminParams, SealHistoryUploadParams, SetRiskMultiplierParams,
                SetUnderlyingRiskParamsParams, UnderlyingParams,
            },
            results::{
                PendingActionKindView, PendingActionView, PendingActionsResult, PriceHistoryResult,
                PricePointView, RiskAuditEntryView, RiskChangeResult, RiskChangeView,
                RiskCountResult, RiskGovernanceResult, RiskIdResult, RiskUnitResult,
                UnderlyingRiskParamsResult, VolatilityResult,
            },
        },
    },
    risk::{
        fixed::Fixed,
        history::{canonical_sha256, PricePoint},
    },
};
use shared::types::{AssetMetrics, DecimalValue};

use crate::utils::{
    pic_canister::PicCanister,
    test_environment::{test_user, TestSetup},
    PicCanisterTrait,
};

/// The default timelock plus a second.
const PAST_TIMELOCK: Duration = Duration::from_secs(48 * 3_600 + 1);

const NS_PER_DAY: u64 = 86_400 * 1_000_000_000;

fn btc_params() -> SetUnderlyingRiskParamsParams {
    SetUnderlyingRiskParamsParams {
        underlying: "btc".to_owned(),
        lambda: DecimalValue::new(94, 2),
        weekend_weight: DecimalValue::new(1, 0),
        floor_ratio: DecimalValue::new(75, 2),
        mpor_days: 3,
        oracle_asset_id: None,
    }
}

/// First date of the synthetic history: 100 days before the current `PocketIC` day, so no point
/// lies in the future.
fn first_date(env: &TestSetup) -> u32 {
    u32::try_from(env.pic.get_time().as_nanos_since_unix_epoch() / NS_PER_DAY).unwrap() - 100
}

/// 40 daily prices from `first_date`, alternating +1% and -1%, in micro-dollars.
fn synthetic_points(first_date: u32) -> Vec<PricePointInput> {
    let mut price: u128 = 100_000_000;
    (0..40_u32)
        .map(|i| {
            if i > 0 {
                price = if i % 2 == 0 {
                    price * 101 / 100
                } else {
                    price * 99 / 100
                };
            }
            PricePointInput {
                date: first_date + i,
                price: DecimalValue::new(price, 6),
            }
        })
        .collect()
}

/// The canonical hash computed off-chain from the same points, as a backfill script would.
fn off_chain_sha256(points: &[PricePointInput]) -> String {
    let points: Vec<PricePoint> = points
        .iter()
        .map(|p| PricePoint {
            date: p.date,
            price: Fixed::from_decimal(&p.price).unwrap(),
        })
        .collect();
    canonical_sha256(&points)
}

/// Thin wrappers over the risk endpoints.
struct Risk<'a> {
    env: &'a TestSetup,
}

impl Risk<'_> {
    fn update<T>(&self, caller: Principal, method: &str, arg: impl CandidType) -> T
    where
        T: for<'de> Deserialize<'de> + CandidType,
    {
        self.env.clearing.update(caller, method, (arg,)).unwrap()
    }

    fn rejected(&self, caller: Principal, method: &str, arg: impl CandidType) -> bool {
        self.env
            .clearing
            .update::<RiskUnitResult, _>(caller, method, (arg,))
            .is_err()
    }

    fn propose_admin(&self, principal: Principal) -> RiskIdResult {
        self.update(
            self.env.controller,
            "add_risk_admin",
            RiskAdminParams { principal },
        )
    }

    fn execute(&self, caller: Principal, action_id: u64) -> RiskUnitResult {
        self.update(
            caller,
            "execute_pending_action",
            PendingActionParams { action_id },
        )
    }

    fn pass_timelock(&self) {
        self.env.pic.advance_time(PAST_TIMELOCK);
        self.env.pic.tick();
    }

    /// Grants the role through the timelock, executed by the controller.
    fn make_admin(&self, principal: Principal) {
        let RiskIdResult::Ok(action) = self.propose_admin(principal) else {
            panic!("add_risk_admin failed");
        };
        self.pass_timelock();
        assert!(matches!(
            self.execute(self.env.controller, action),
            RiskUnitResult::Ok
        ));
    }

    fn set_k(&self, caller: Principal, k: DecimalValue) -> RiskChangeResult {
        self.update(
            caller,
            "set_risk_multiplier",
            SetRiskMultiplierParams {
                underlying: "BTC".to_owned(),
                k,
            },
        )
    }

    fn pending(&self) -> Vec<PendingActionView> {
        let PendingActionsResult::Ok(pending) = self
            .env
            .clearing
            .query(self.env.user, "list_pending_actions", ())
            .unwrap()
        else {
            panic!("list_pending_actions failed");
        };
        pending
    }

    fn history(&self, from_date: Option<u32>) -> Vec<PricePointView> {
        let PriceHistoryResult::Ok(points) = self
            .env
            .clearing
            .query(
                self.env.user,
                "get_price_history",
                (GetPriceHistoryParams {
                    underlying: "BTC".to_owned(),
                    from_date,
                    limit: None,
                },),
            )
            .unwrap()
        else {
            panic!("get_price_history failed");
        };
        points
    }

    fn k(&self) -> DecimalValue {
        let UnderlyingRiskParamsResult::Ok(params) = self
            .env
            .clearing
            .query(self.env.user, "list_underlying_risk_params", ())
            .unwrap()
        else {
            panic!("list_underlying_risk_params failed");
        };
        params[0].k.clone()
    }

    /// Uploads, seals and activates the synthetic history through the timelock.
    fn backfill(&self, admin: Principal) {
        let RiskIdResult::Ok(upload_id) = self.update::<RiskIdResult>(
            admin,
            "begin_history_upload",
            BeginHistoryUploadParams {
                underlying: "BTC".to_owned(),
                source: "test".to_owned(),
                series_id: "synthetic".to_owned(),
            },
        ) else {
            panic!("begin_history_upload failed");
        };
        let points = synthetic_points(first_date(self.env));
        assert!(matches!(
            self.update::<RiskCountResult>(
                admin,
                "append_history_chunk",
                AppendHistoryChunkParams {
                    upload_id,
                    points: points.clone(),
                },
            ),
            RiskCountResult::Ok(40)
        ));
        let RiskIdResult::Ok(activation) = self.update::<RiskIdResult>(
            admin,
            "seal_history_upload",
            SealHistoryUploadParams {
                upload_id,
                sha256_hex: off_chain_sha256(&points),
            },
        ) else {
            panic!("the off-chain hash must seal the upload");
        };
        self.pass_timelock();
        assert!(matches!(
            self.execute(admin, activation),
            RiskUnitResult::Ok
        ));
    }
}

#[test]
fn risk_engine_lifecycle_through_the_timelock() {
    let env = TestSetup::default();
    let risk = Risk { env: &env };
    let admin = test_user(60);
    let outsider = test_user(61);

    // Nobody holds the role yet, not even a controller, and a controller can never be granted it.
    assert!(risk.rejected(env.controller, "set_underlying_risk_params", btc_params()));
    assert!(matches!(
        risk.propose_admin(env.controller),
        RiskIdResult::Err(RiskError::InvalidPrincipal(_))
    ));

    // Adding a risk administrator waits behind the timelock; only admins and controllers execute.
    let RiskIdResult::Ok(add_admin) = risk.propose_admin(admin) else {
        panic!("add_risk_admin failed");
    };
    assert_eq!(risk.pending().len(), 1);
    assert!(matches!(
        risk.execute(env.controller, add_admin),
        RiskUnitResult::Err(RiskError::TimelockNotExpired { .. })
    ));
    risk.pass_timelock();
    assert!(risk.rejected(
        outsider,
        "execute_pending_action",
        PendingActionParams {
            action_id: add_admin
        }
    ));
    assert!(matches!(
        risk.execute(env.controller, add_admin),
        RiskUnitResult::Ok
    ));
    let RiskGovernanceResult::Ok(governance) = env
        .clearing
        .query(outsider, "get_risk_governance", ())
        .unwrap()
    else {
        panic!("get_risk_governance failed");
    };
    assert_eq!(governance.risk_admins, vec![admin]);

    // Parameters within bounds; the multiplier rises at once and falls only through the timelock.
    assert!(matches!(
        risk.update::<RiskUnitResult>(admin, "set_underlying_risk_params", btc_params()),
        RiskUnitResult::Ok
    ));
    assert!(matches!(
        risk.set_k(admin, DecimalValue::new(2, 0)),
        RiskChangeResult::Ok(RiskChangeView::Applied)
    ));
    assert!(matches!(
        risk.set_k(admin, DecimalValue::new(15, 1)),
        RiskChangeResult::Ok(RiskChangeView::Pending { .. })
    ));
    assert!(matches!(
        risk.set_k(admin, DecimalValue::new(4, 0)),
        RiskChangeResult::Err(RiskError::OutOfBounds { .. })
    ));
    assert!(env
        .clearing
        .update::<RiskChangeResult, _>(
            outsider,
            "set_risk_multiplier",
            (SetRiskMultiplierParams {
                underlying: "BTC".to_owned(),
                k: DecimalValue::new(2, 0),
            },),
        )
        .is_err());

    // A controller raise is an emergency brake: it applies at once and withdraws the pending
    // lowering. A controller cannot lower.
    assert!(matches!(
        risk.set_k(env.controller, DecimalValue::new(25, 1)),
        RiskChangeResult::Ok(RiskChangeView::Applied)
    ));
    assert!(risk.pending().is_empty());
    assert!(matches!(
        risk.set_k(env.controller, DecimalValue::new(2, 0)),
        RiskChangeResult::Err(RiskError::Unauthorized(_))
    ));

    // Backfill: a wrong hash is rejected without revealing the computed one; the points of a sealed
    // upload can be read during the timelock.
    let RiskIdResult::Ok(upload_id) = risk.update::<RiskIdResult>(
        admin,
        "begin_history_upload",
        BeginHistoryUploadParams {
            underlying: "BTC".to_owned(),
            source: "test".to_owned(),
            series_id: "synthetic".to_owned(),
        },
    ) else {
        panic!("begin_history_upload failed");
    };
    let start = first_date(&env);
    let future = vec![PricePointInput {
        date: start + 101,
        price: DecimalValue::new(1, 0),
    }];
    assert!(matches!(
        risk.update::<RiskCountResult>(
            admin,
            "append_history_chunk",
            AppendHistoryChunkParams {
                upload_id,
                points: future,
            },
        ),
        RiskCountResult::Err(RiskError::InvalidHistoryChunk(_))
    ));
    let points = synthetic_points(start);
    assert!(matches!(
        risk.update::<RiskCountResult>(
            admin,
            "append_history_chunk",
            AppendHistoryChunkParams {
                upload_id,
                points: points.clone(),
            },
        ),
        RiskCountResult::Ok(40)
    ));
    let seal = |sha256_hex: String| -> RiskIdResult {
        risk.update(
            admin,
            "seal_history_upload",
            SealHistoryUploadParams {
                upload_id,
                sha256_hex,
            },
        )
    };
    assert!(matches!(
        seal("0".repeat(64)),
        RiskIdResult::Err(RiskError::HashMismatch)
    ));
    let RiskIdResult::Ok(activation) = seal(off_chain_sha256(&points)) else {
        panic!("the off-chain hash must seal the upload");
    };
    let PriceHistoryResult::Ok(page) = env
        .clearing
        .query(
            outsider,
            "get_history_upload_points",
            (GetHistoryUploadPointsParams {
                upload_id,
                from_date: Some(start + 10),
                limit: Some(5),
            },),
        )
        .unwrap()
    else {
        panic!("get_history_upload_points failed");
    };
    assert_eq!(
        page.iter().map(|p| p.date).collect::<Vec<_>>(),
        (start + 10..start + 15).collect::<Vec<_>>()
    );

    let volatility = |caller| -> VolatilityResult {
        env.clearing
            .query(
                caller,
                "get_volatility",
                (UnderlyingParams {
                    underlying: "BTC".to_owned(),
                },),
            )
            .unwrap()
    };
    assert!(matches!(
        volatility(outsider),
        VolatilityResult::Err(RiskError::InsufficientHistory { .. })
    ));

    risk.pass_timelock();
    assert!(matches!(
        risk.execute(admin, activation),
        RiskUnitResult::Ok
    ));
    assert_eq!(risk.history(None).len(), 40);

    let VolatilityResult::Ok(reading) = volatility(outsider) else {
        panic!("get_volatility failed");
    };
    assert_eq!(reading.returns, 39);
    // Fewer than 250 returns: EWMA only, no long run and no floor.
    assert!(reading.is_provisional);
    assert_eq!(reading.sigma_long_run, None);
    assert_eq!(reading.sigma_used, None);
    // Constant 1% daily moves: volatility close to 1% (12 decimals).
    assert!((9_900_000_000..=10_100_000_000).contains(&reading.sigma_ewma.value));
    assert_eq!(
        reading.k_effective,
        DecimalValue::new(2_500_000_000_000, 12)
    );

    // Removing an administrator takes effect at once.
    assert!(matches!(
        risk.update::<RiskUnitResult>(
            env.controller,
            "remove_risk_admin",
            RiskAdminParams { principal: admin },
        ),
        RiskUnitResult::Ok
    ));
    assert!(risk.rejected(admin, "set_underlying_risk_params", btc_params()));

    // Every change is in the public audit log, with who did it and what.
    let audit: Vec<RiskAuditEntryView> = env
        .clearing
        .query(
            outsider,
            "list_risk_audit",
            (ListRiskAuditParams {
                from_seq: None,
                limit: None,
            },),
        )
        .unwrap();
    assert!(audit.iter().enumerate().all(|(i, e)| e.seq == i as u64));
    let find = |prefix: &str| {
        audit
            .iter()
            .find(|e| e.description.starts_with(prefix))
            .unwrap_or_else(|| panic!("no audit entry starting with {prefix:?}"))
    };
    assert_eq!(
        find(&format!("proposed action {add_admin}: AddRiskAdmin")).caller,
        env.controller
    );
    assert_eq!(find("set BTC lambda=").caller, admin);
    assert_eq!(
        find("raised BTC k from 2.000000000000 to 2.500000000000").caller,
        env.controller
    );
    assert!(find("withdrew action")
        .description
        .ends_with("superseded by a raise"));
    assert_eq!(find(&format!("executed action {activation}")).caller, admin);
    assert_eq!(
        find(&format!("removed risk administrator {admin}")).caller,
        env.controller
    );
}

/// Pending actions and the history survive an upgrade, and the sampler stays off after it.
#[test]
fn risk_state_survives_an_upgrade_and_the_sampler_stays_off() {
    let env = TestSetup::default();
    let risk = Risk { env: &env };
    let admin = test_user(62);
    risk.make_admin(admin);

    // Feed BTC from an oracle asset with a fresh price: an armed sampler would record it.
    let mut params = btc_params();
    params.oracle_asset_id = Some("SMP".to_owned());
    assert!(matches!(
        risk.update::<RiskUnitResult>(admin, "set_underlying_risk_params", params),
        RiskUnitResult::Ok
    ));
    risk.backfill(admin);
    assert!(matches!(
        risk.set_k(admin, DecimalValue::new(2, 0)),
        RiskChangeResult::Ok(RiskChangeView::Applied)
    ));
    let RiskChangeResult::Ok(RiskChangeView::Pending {
        action_id: lowering,
        ..
    }) = risk.set_k(admin, DecimalValue::new(15, 1))
    else {
        panic!("lowering k must be pending");
    };

    let wasm = fs::read(PicCanister::cargo_wasm_path("clearing")).unwrap();
    env.pic
        .upgrade_canister(
            env.clearing.canister_id(),
            wasm,
            encode_args(()).unwrap(),
            Some(env.controller),
        )
        .unwrap();

    let pending = risk.pending();
    assert_eq!(pending.len(), 1);
    assert!(matches!(
        &pending[0].action,
        PendingActionKindView::LowerRiskMultiplier { underlying, .. } if underlying == "BTC"
    ));
    assert_eq!(risk.history(None).len(), 40);

    let now_ns = env.pic.get_time().as_nanos_since_unix_epoch();
    env.clearing
        .update::<(), _>(
            env.controller,
            "update_asset_metrics",
            (UpdateAssetMetricsParams {
                asset_id: "SMP".to_owned(),
                metrics: AssetMetrics {
                    price_usd: DecimalValue::new(105_000, 0),
                    latest_transfer_fee: None,
                    haircut_bps: 0,
                    insurance_fee_ratio: None,
                    protocol_fee_ratio: None,
                    last_updated_ns: Some(now_ns),
                },
            },),
        )
        .unwrap();
    env.pic.advance_time(Duration::from_hours(2));
    env.pic.tick();
    env.pic.tick();
    let today = u32::try_from(now_ns / NS_PER_DAY).unwrap();
    assert!(
        risk.history(Some(today)).is_empty(),
        "the sampler must not run in phase 1"
    );

    // The pending lowering still executes after the upgrade.
    risk.pass_timelock();
    assert!(matches!(risk.execute(admin, lowering), RiskUnitResult::Ok));
    assert_eq!(risk.k(), DecimalValue::new(1_500_000_000_000, 12));
}
