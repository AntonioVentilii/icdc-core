use candid::Nat;
use clearing::{
    api::{
        account::{params::GetAccountStateParams, results::GetAccountStateResult},
        migration::{params::MigrateDomainParams, results::MigrateDomainResult},
        trade::{errors::TradeError, results::SubmitMatchedTradeResult},
    },
    types::trade::Side,
};
use shared::types::BalanceDomain;

use crate::utils::{
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait,
    PicCanisterTrait,
};

/// Playground orders are now rejected when the user has insufficient margin,
/// proving risk parity with Settlement.
#[test]
fn playground_enforces_margin() {
    let env = TestSetup::with_icp();
    let user_a = test_user(56);

    let series_id = env.add_binary_series("RISK-PG", 1_000_000, BalanceDomain::Playground);

    // User A has NO collateral in Playground → 1000-unit buy should fail
    let res = env.submit_limit_order(
        user_a,
        "pg_huge_order",
        series_id.clone(),
        Side::Buy,
        1000,
        500_000,
    );

    match res {
        SubmitMatchedTradeResult::Err(TradeError::InsufficientMargin { .. }) => {}
        other => {
            panic!("Expected InsufficientMargin for Playground order without funds, got: {other:?}")
        }
    }

    // Deposit collateral into Playground
    env.deposit_collateral(
        user_a,
        "ICP",
        Nat::from(10_000_000_000_u128), // 100 ICP = $1500
        Some(BalanceDomain::Playground),
    );
    env.pic.tick();

    // Now a small order should succeed
    let res = env.submit_limit_order(user_a, "pg_small_order", series_id, Side::Buy, 1, 500_000);
    assert!(
        matches!(res, SubmitMatchedTradeResult::Ok(_)),
        "Playground order should succeed with sufficient margin"
    );
}

/// Migrate collateral from Settlement to Playground, then verify
/// the user can trade in Playground and cannot in Settlement.
#[test]
fn migrate_domain_balances() {
    let env = TestSetup::with_icp();
    let user_a = test_user(57);

    // Deposit into Settlement
    let deposit_amount = Nat::from(10_000_000_000_u128); // 100 ICP
    env.deposit_collateral(
        user_a,
        "ICP",
        deposit_amount,
        Some(BalanceDomain::Settlement),
    );
    env.pic.tick();

    // Verify Settlement has balance
    let state_before: GetAccountStateResult = env
        .clearing
        .update(
            user_a,
            "get_account_state",
            (GetAccountStateParams {
                refresh: None,
                domain: Some(BalanceDomain::Settlement),
            },),
        )
        .unwrap();

    match &state_before {
        GetAccountStateResult::Ok(resp) => {
            assert!(
                resp.total_equity_usd > Nat::from(0_u64),
                "Should have equity in Settlement before migration"
            );
        }
        GetAccountStateResult::Err(e) => panic!("Failed: {e:?}"),
    }

    // Migrate Settlement → Playground
    let mig_res: MigrateDomainResult = env
        .clearing
        .update(
            user_a,
            "migrate_domain",
            (MigrateDomainParams {
                migration_id: "it_mig_1".to_owned(),
                from_domain: BalanceDomain::Settlement,
                to_domain: BalanceDomain::Playground,
            },),
        )
        .unwrap();

    match mig_res {
        MigrateDomainResult::Ok => {}
        MigrateDomainResult::Err(e) => panic!("Migration failed: {e:?}"),
    }

    // Verify Settlement is drained
    let state_after: GetAccountStateResult = env
        .clearing
        .update(
            user_a,
            "get_account_state",
            (GetAccountStateParams {
                refresh: None,
                domain: Some(BalanceDomain::Settlement),
            },),
        )
        .unwrap();

    match &state_after {
        GetAccountStateResult::Ok(resp) => {
            assert_eq!(
                resp.total_equity_usd,
                Nat::from(0_u64),
                "Settlement equity should be 0 after migration"
            );
        }
        GetAccountStateResult::Err(e) => panic!("Failed: {e:?}"),
    }

    // Verify Playground has balance
    let state_pg: GetAccountStateResult = env
        .clearing
        .update(
            user_a,
            "get_account_state",
            (GetAccountStateParams {
                refresh: None,
                domain: Some(BalanceDomain::Playground),
            },),
        )
        .unwrap();

    match &state_pg {
        GetAccountStateResult::Ok(resp) => {
            assert!(
                resp.total_equity_usd > Nat::from(0_u64),
                "Should have equity in Playground after migration"
            );
        }
        GetAccountStateResult::Err(e) => panic!("Failed: {e:?}"),
    }

    // Should NOT be able to trade in Settlement without collateral
    let series_set = env.add_binary_series("MIG-SET", 1_000_000, BalanceDomain::Settlement);
    let res = env.submit_limit_order(user_a, "set_after_mig", series_set, Side::Buy, 1, 500_000);
    match res {
        SubmitMatchedTradeResult::Err(TradeError::InsufficientMargin { .. }) => {}
        other => {
            panic!("Expected InsufficientMargin in Settlement after migration, got: {other:?}")
        }
    }

    // Should be able to trade in Playground
    let series_pg = env.add_binary_series("MIG-PG", 1_000_000, BalanceDomain::Playground);
    let res = env.submit_limit_order(user_a, "pg_after_mig", series_pg, Side::Buy, 1, 500_000);
    assert!(
        matches!(res, SubmitMatchedTradeResult::Ok(_)),
        "Playground order should succeed after migration"
    );
}

/// Calling `migrate_domain` with the same `migration_id` twice should succeed
/// without doubling balances.
#[test]
fn migrate_domain_idempotent() {
    let env = TestSetup::with_icp();
    let user_a = test_user(58);

    let deposit_amount = Nat::from(10_000_000_000_u128);
    env.deposit_collateral(
        user_a,
        "ICP",
        deposit_amount,
        Some(BalanceDomain::Settlement),
    );
    env.pic.tick();

    let params = MigrateDomainParams {
        migration_id: "it_idem".to_owned(),
        from_domain: BalanceDomain::Settlement,
        to_domain: BalanceDomain::Playground,
    };

    // First call
    let r1: MigrateDomainResult = env
        .clearing
        .update(user_a, "migrate_domain", (params.clone(),))
        .unwrap();
    assert!(matches!(r1, MigrateDomainResult::Ok));

    // Second call — same id
    let r2: MigrateDomainResult = env
        .clearing
        .update(user_a, "migrate_domain", (params,))
        .unwrap();
    assert!(matches!(r2, MigrateDomainResult::Ok));

    // Verify Playground equity is the original amount, not doubled
    let state_pg: GetAccountStateResult = env
        .clearing
        .update(
            user_a,
            "get_account_state",
            (GetAccountStateParams {
                refresh: None,
                domain: Some(BalanceDomain::Playground),
            },),
        )
        .unwrap();

    match state_pg {
        GetAccountStateResult::Ok(resp) => {
            // 100 ICP at $15, 2% haircut = $1,470 (4-decimal USD)
            assert_eq!(
                resp.total_equity_usd,
                Nat::from(14_700_000_u128),
                "Equity should equal original deposit value (not doubled)"
            );
        }
        GetAccountStateResult::Err(e) => panic!("Failed: {e:?}"),
    }
}
