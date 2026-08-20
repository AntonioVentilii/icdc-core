use candid::{Nat, Principal};
use clearing::{
    api::{
        account::{
            errors::AccountStateError,
            params::{GetAccountStateParams, GetPositionParams},
            results::GetAccountStateResult,
        },
        admin::{errors::ReassignAccountError, results::ReassignAccountResult},
        trade::{params::SubmitMarketOrderParams, results::SubmitMatchedTradeResult},
    },
    types::{
        margin::Position,
        trade::{OrderId, Side, TradeId},
    },
};
use shared::types::{BalanceDomain, SeriesId};

use crate::utils::{
    assertions::assert_unauthorized,
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait,
    PicCanisterTrait,
};

/// A principal with no ledger balances and no prior clearing state, standing in
/// for a freshly derived signing key that takes over an existing account.
fn fresh_owner(id: u8) -> Principal {
    Principal::from_slice(&[id, 7, 7])
}

fn reassign(
    env: &TestSetup,
    caller: Principal,
    old_owner: Principal,
    new_owner: Principal,
) -> Result<ReassignAccountResult, String> {
    env.clearing
        .update(caller, "admin_reassign_account", (old_owner, new_owner))
}

fn settlement_equity(env: &TestSetup, caller: Principal) -> GetAccountStateResult {
    env.clearing
        .update(
            caller,
            "get_account_state",
            (GetAccountStateParams {
                refresh: None,
                domain: Some(BalanceDomain::Settlement),
            },),
        )
        .unwrap()
}

fn get_position(env: &TestSetup, caller: Principal, series_id: SeriesId) -> Option<Position> {
    env.clearing
        .query(
            caller,
            "get_position",
            (GetPositionParams {
                series_id,
                outcome_id: None,
            },),
        )
        .unwrap()
}

#[test]
fn admin_reassign_account_rejects_non_controller() {
    let env = TestSetup::default();

    let res = reassign(&env, env.user, test_user(54), fresh_owner(90));

    assert_unauthorized(&res);
}

#[test]
fn admin_reassign_account_moves_balances_and_positions() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let user_b = test_user(55);
    let new_owner = fresh_owner(91);

    let deposit = Nat::from(10_000_000_000_u128); // 100 ICP
    env.deposit_collateral(user_a, "ICP", deposit.clone(), None);
    env.deposit_collateral(user_b, "ICP", deposit, None);
    env.pic.tick();

    // Open a position for user A by fully matching A's resting sell against a
    // market order from user B, so A ends up with a position but no resting order.
    let series_id = env.add_binary_series("REASSIGN", 1_000_000, BalanceDomain::Settlement);
    let res = env.submit_limit_order(
        user_a,
        "reassign_sell",
        series_id.clone(),
        Side::Sell,
        1,
        500_000,
    );
    assert!(matches!(res, SubmitMatchedTradeResult::Ok(_)));
    let matched: SubmitMatchedTradeResult = env
        .clearing
        .update(
            user_b,
            "submit_market_order",
            (SubmitMarketOrderParams {
                trade_id: TradeId::from("reassign_match".to_owned()),
                matching_order_id: OrderId::from("reassign_sell".to_owned()),
                qty: None,
            },),
        )
        .unwrap();
    assert!(matches!(matched, SubmitMatchedTradeResult::Ok(_)));
    env.pic.tick();

    let equity_before = match settlement_equity(&env, user_a) {
        GetAccountStateResult::Ok(resp) => resp.total_equity_usd,
        GetAccountStateResult::Err(e) => panic!("old owner state read failed: {e:?}"),
    };
    assert!(equity_before > Nat::from(0_u64));
    assert!(get_position(&env, user_a, series_id.clone()).is_some());

    let res = reassign(&env, env.controller, user_a, new_owner).unwrap();
    assert!(matches!(res, ReassignAccountResult::Ok), "got {res:?}");

    // The new owner holds the full account: same equity, same position.
    match settlement_equity(&env, new_owner) {
        GetAccountStateResult::Ok(resp) => assert_eq!(resp.total_equity_usd, equity_before),
        GetAccountStateResult::Err(e) => panic!("new owner state read failed: {e:?}"),
    }
    let position = get_position(&env, new_owner, series_id.clone()).expect("position must move");
    assert_eq!(position.net_qty, -1);

    // The old owner is fully drained: no account state, no positions.
    match settlement_equity(&env, user_a) {
        GetAccountStateResult::Err(AccountStateError::NoAccountStateFound) => {}
        other => panic!("expected NoAccountStateFound for old owner, got {other:?}"),
    }
    assert!(get_position(&env, user_a, series_id).is_none());
}

#[test]
fn admin_reassign_account_rejects_open_orders() {
    let env = TestSetup::with_icp();
    let user_a = test_user(56);

    env.deposit_collateral(user_a, "ICP", Nat::from(10_000_000_000_u128), None);
    env.pic.tick();

    // A resting order with no counterparty stays on the book.
    let series_id = env.add_binary_series("REASSIGN-ORD", 1_000_000, BalanceDomain::Settlement);
    let res = env.submit_limit_order(user_a, "reassign_rest", series_id, Side::Buy, 1, 500_000);
    assert!(matches!(res, SubmitMatchedTradeResult::Ok(_)));

    let res = reassign(&env, env.controller, user_a, fresh_owner(92)).unwrap();
    assert!(
        matches!(
            res,
            ReassignAccountResult::Err(ReassignAccountError::OpenOrdersExist)
        ),
        "got {res:?}"
    );

    // Nothing moved: the old owner still has their account.
    assert!(matches!(
        settlement_equity(&env, user_a),
        GetAccountStateResult::Ok(_)
    ));
}

#[test]
fn admin_reassign_account_rejects_occupied_target() {
    let env = TestSetup::with_icp();
    let user_a = test_user(57);
    let user_b = test_user(58);

    env.deposit_collateral(user_a, "ICP", Nat::from(10_000_000_000_u128), None);
    env.deposit_collateral(user_b, "ICP", Nat::from(10_000_000_000_u128), None);
    env.pic.tick();

    let res = reassign(&env, env.controller, user_a, user_b).unwrap();
    assert!(
        matches!(
            res,
            ReassignAccountResult::Err(ReassignAccountError::TargetAccountNotEmpty)
        ),
        "got {res:?}"
    );

    // No implicit merge happened: both accounts still stand on their own.
    assert!(matches!(
        settlement_equity(&env, user_a),
        GetAccountStateResult::Ok(_)
    ));
    assert!(matches!(
        settlement_equity(&env, user_b),
        GetAccountStateResult::Ok(_)
    ));
}

#[test]
fn admin_reassign_account_second_call_fails_cleanly() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let new_owner = fresh_owner(93);

    env.deposit_collateral(user_a, "ICP", Nat::from(10_000_000_000_u128), None);
    env.pic.tick();

    let res = reassign(&env, env.controller, user_a, new_owner).unwrap();
    assert!(matches!(res, ReassignAccountResult::Ok), "got {res:?}");

    let equity_after = match settlement_equity(&env, new_owner) {
        GetAccountStateResult::Ok(resp) => resp.total_equity_usd,
        GetAccountStateResult::Err(e) => panic!("new owner state read failed: {e:?}"),
    };

    // Replaying the call finds no source account and mutates nothing.
    let res = reassign(&env, env.controller, user_a, new_owner).unwrap();
    assert!(
        matches!(
            res,
            ReassignAccountResult::Err(ReassignAccountError::AccountNotFound)
        ),
        "got {res:?}"
    );

    match settlement_equity(&env, new_owner) {
        GetAccountStateResult::Ok(resp) => assert_eq!(resp.total_equity_usd, equity_after),
        GetAccountStateResult::Err(e) => panic!("new owner state read failed: {e:?}"),
    }
}
