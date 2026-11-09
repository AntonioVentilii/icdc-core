use core::time::Duration;

use clearing::{
    api::risk::{
        errors::RiskError,
        params::{
            AppendHistoryChunkParams, BeginHistoryUploadParams, GetPriceHistoryParams,
            ListRiskAuditParams, PricePointInput, RiskAdminParams, SealHistoryUploadParams,
            SetRiskMultiplierParams, SetUnderlyingRiskParamsParams,
        },
        results::{
            PendingActionView, PriceHistoryResult, RiskChangeResult, RiskCountResult,
            RiskGovernanceView, RiskIdResult, RiskUnitResult, VolatilityResult,
        },
    },
    risk::state::{RiskAuditEntry, RiskChangeOutcome},
};
use shared::types::DecimalValue;

use crate::utils::{
    test_environment::{test_user, TestSetup},
    PicCanisterTrait,
};

/// The default timelock plus a second.
const PAST_TIMELOCK: Duration = Duration::from_secs(48 * 3_600 + 1);

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

/// 40 daily prices alternating +1% and -1%, in micro-dollars.
fn synthetic_points() -> Vec<PricePointInput> {
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
                date: 20_000 + i,
                price: DecimalValue::new(price, 6),
            }
        })
        .collect()
}

#[test]
fn risk_engine_lifecycle_through_the_timelock() {
    let env = TestSetup::default();
    let admin = test_user(60);
    let outsider = test_user(61);

    // Nobody holds the role yet, not even a controller.
    assert!(env
        .clearing
        .update::<RiskUnitResult, _>(
            env.controller,
            "set_underlying_risk_params",
            (btc_params(),)
        )
        .is_err());

    // Adding a risk administrator waits behind the timelock.
    let RiskIdResult::Ok(add_admin) = env
        .clearing
        .update::<RiskIdResult, _>(
            env.controller,
            "add_risk_admin",
            (RiskAdminParams { principal: admin },),
        )
        .unwrap()
    else {
        panic!("add_risk_admin failed");
    };
    let pending: Vec<PendingActionView> = env
        .clearing
        .query(outsider, "list_pending_actions", ())
        .unwrap();
    assert_eq!(pending.len(), 1);
    let early: RiskUnitResult = env
        .clearing
        .update(outsider, "execute_pending_action", (add_admin,))
        .unwrap();
    assert!(matches!(
        early,
        RiskUnitResult::Err(RiskError::TimelockNotExpired { .. })
    ));

    env.pic.advance_time(PAST_TIMELOCK);
    env.pic.tick();
    let executed: RiskUnitResult = env
        .clearing
        .update(outsider, "execute_pending_action", (add_admin,))
        .unwrap();
    assert!(matches!(executed, RiskUnitResult::Ok));
    let governance: RiskGovernanceView = env
        .clearing
        .query(outsider, "get_risk_governance", ())
        .unwrap();
    assert_eq!(governance.risk_admins, vec![admin]);

    // Parameters within bounds; the multiplier rises at once and falls only through the timelock.
    let set: RiskUnitResult = env
        .clearing
        .update(admin, "set_underlying_risk_params", (btc_params(),))
        .unwrap();
    assert!(matches!(set, RiskUnitResult::Ok));
    let multiplier = |k: DecimalValue| -> RiskChangeResult {
        env.clearing
            .update(
                admin,
                "set_risk_multiplier",
                (SetRiskMultiplierParams {
                    underlying: "BTC".to_owned(),
                    k,
                },),
            )
            .unwrap()
    };
    assert!(matches!(
        multiplier(DecimalValue::new(2, 0)),
        RiskChangeResult::Ok(RiskChangeOutcome::Applied)
    ));
    assert!(matches!(
        multiplier(DecimalValue::new(15, 1)),
        RiskChangeResult::Ok(RiskChangeOutcome::Pending { .. })
    ));
    assert!(matches!(
        multiplier(DecimalValue::new(4, 0)),
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

    // Backfill: upload, hash check, timelocked activation.
    let RiskIdResult::Ok(upload_id) = env
        .clearing
        .update::<RiskIdResult, _>(
            admin,
            "begin_history_upload",
            (BeginHistoryUploadParams {
                underlying: "BTC".to_owned(),
                source: "test".to_owned(),
                series_id: "synthetic".to_owned(),
            },),
        )
        .unwrap()
    else {
        panic!("begin_history_upload failed");
    };
    let appended: RiskCountResult = env
        .clearing
        .update(
            admin,
            "append_history_chunk",
            (AppendHistoryChunkParams {
                upload_id,
                points: synthetic_points(),
            },),
        )
        .unwrap();
    assert!(matches!(appended, RiskCountResult::Ok(40)));

    let seal = |sha256_hex: String| -> RiskIdResult {
        env.clearing
            .update(
                admin,
                "seal_history_upload",
                (SealHistoryUploadParams {
                    upload_id,
                    sha256_hex,
                },),
            )
            .unwrap()
    };
    let RiskIdResult::Err(RiskError::HashMismatch { computed }) = seal("0".repeat(64)) else {
        panic!("a wrong hash must be rejected");
    };
    let RiskIdResult::Ok(activation) = seal(computed) else {
        panic!("the matching hash must seal the upload");
    };

    let before: VolatilityResult = env
        .clearing
        .query(outsider, "get_volatility", ("BTC".to_owned(),))
        .unwrap();
    assert!(matches!(
        before,
        VolatilityResult::Err(RiskError::InsufficientHistory { .. })
    ));

    env.pic.advance_time(PAST_TIMELOCK);
    env.pic.tick();
    let activated: RiskUnitResult = env
        .clearing
        .update(outsider, "execute_pending_action", (activation,))
        .unwrap();
    assert!(matches!(activated, RiskUnitResult::Ok));

    let history: PriceHistoryResult = env
        .clearing
        .query(
            outsider,
            "get_price_history",
            (GetPriceHistoryParams {
                underlying: "BTC".to_owned(),
                from_date: None,
                limit: None,
            },),
        )
        .unwrap();
    let PriceHistoryResult::Ok(points) = history else {
        panic!("get_price_history failed");
    };
    assert_eq!(points.len(), 40);

    let VolatilityResult::Ok(volatility) = env
        .clearing
        .query::<VolatilityResult, _>(outsider, "get_volatility", ("BTC".to_owned(),))
        .unwrap()
    else {
        panic!("get_volatility failed");
    };
    assert_eq!(volatility.returns, 39);
    // Constant 1% daily moves: volatility close to 1% (12 decimals).
    assert!((9_900_000_000..=10_100_000_000).contains(&volatility.sigma_ewma.value));
    assert_eq!(
        volatility.k_effective,
        DecimalValue::new(2_000_000_000_000, 12)
    );

    // Every change is in the public audit log.
    let audit: Vec<RiskAuditEntry> = env
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
    assert!(audit.len() >= 8);
}
