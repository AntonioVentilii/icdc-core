use candid::{Nat, Principal};
use clearing::{
    api::{
        account::{
            errors::AccountStateError,
            params::{GetAccountStateParams, GetPositionParams},
            results::GetAccountStateResult,
        },
        admin::{
            errors::ReassignAccountError, params::ReassignAccountParams,
            results::ReassignAccountResult,
        },
        collateral::{
            errors::DepositCollateralError,
            params::{DepositCollateralParams, WithdrawCollateralParams},
            results::{DepositCollateralResult, WithdrawCollateralResult},
        },
        trade::{
            errors::TradeError,
            params::{FreezePositionForTransferParams, SubmitMarketOrderParams},
            results::SubmitMatchedTradeResult,
        },
    },
    types::{
        margin::Position,
        state::PositionProof,
        trade::{OrderId, Side, TradeId, TransferId},
        user::{DepositId, User, WithdrawalId},
    },
    utils::account::derive_user_subaccount_for_canister,
};
use icrc_ledger_types::icrc1::account::Account;
use shared::types::{BalanceDomain, SeriesId};

use crate::utils::{
    assertions::assert_unauthorized,
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait,
    PicCanisterTrait,
};

/// The `ICP` ledger fee the test environment deploys with.
const ICP_FEE: u128 = 10_000;

/// A principal with no ledger balances and no prior clearing state, standing in
/// for a freshly derived signing key that takes over an existing account.
fn fresh_owner(id: u8) -> Principal {
    Principal::from_slice(&[id, 7, 7])
}

fn reassign(
    env: &TestSetup,
    caller: Principal,
    reassignment_id: &str,
    old_owner: Principal,
    new_owner: Principal,
) -> Result<ReassignAccountResult, String> {
    env.clearing.update(
        caller,
        "admin_reassign_account",
        (ReassignAccountParams {
            reassignment_id: reassignment_id.to_owned(),
            old_owner,
            new_owner,
        },),
    )
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

fn settlement_balance(env: &TestSetup, caller: Principal, asset_id: &str) -> u128 {
    match settlement_equity(env, caller) {
        GetAccountStateResult::Ok(resp) => resp
            .state
            .get_balance(BalanceDomain::Settlement, &asset_id.to_owned()),
        GetAccountStateResult::Err(e) => panic!("account state read failed: {e:?}"),
    }
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

/// The clearing canister's custody subaccount for `owner`, i.e. where that
/// principal's deposits physically sit on the ledger.
fn custody_account(env: &TestSetup, owner: Principal) -> Account {
    let clearing_id = env.clearing.canister_id();

    Account {
        owner: clearing_id,
        subaccount: Some(derive_user_subaccount_for_canister(clearing_id, owner)),
    }
}

fn ledger_balance(env: &TestSetup, asset_id: &str, account: Account) -> u128 {
    let balance: Nat = env
        .ledgers
        .get(asset_id)
        .expect("ledger not found")
        .query(env.controller, "icrc1_balance_of", (account,))
        .expect("icrc1_balance_of failed");

    balance.0.try_into().expect("balance does not fit in u128")
}

/// Opens a position for `seller` by fully matching their resting sell against a
/// market order from `buyer`, so the seller ends up with a position and no
/// resting order.
fn open_matched_position(
    env: &TestSetup,
    seller: Principal,
    buyer: Principal,
    underlying: &str,
) -> SeriesId {
    let series_id = env.add_binary_series(underlying, 1_000_000, BalanceDomain::Settlement);
    let order_id = format!("{underlying}_sell");

    let res = env.submit_limit_order(seller, &order_id, series_id.clone(), Side::Sell, 1, 500_000);
    assert!(matches!(res, SubmitMatchedTradeResult::Ok(_)));

    let matched: SubmitMatchedTradeResult = env
        .clearing
        .update(
            buyer,
            "submit_market_order",
            (SubmitMarketOrderParams {
                trade_id: TradeId::from(format!("{underlying}_match")),
                matching_order_id: OrderId::from(order_id),
                qty: None,
            },),
        )
        .unwrap();
    assert!(matches!(matched, SubmitMatchedTradeResult::Ok(_)));
    env.pic.tick();

    series_id
}

#[test]
fn admin_reassign_account_rejects_non_controller() {
    let env = TestSetup::default();

    let res = reassign(&env, env.user, "reassign_1", test_user(54), fresh_owner(90));

    assert_unauthorized(&res);
}

#[test]
fn admin_reassign_account_rejects_anonymous_owner() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);

    env.deposit_collateral(user_a, "ICP", Nat::from(10_000_000_000_u128), None);
    env.pic.tick();

    let res = reassign(
        &env,
        env.controller,
        "reassign_anon",
        user_a,
        Principal::anonymous(),
    )
    .unwrap();
    assert!(
        matches!(
            res,
            ReassignAccountResult::Err(ReassignAccountError::AnonymousOwner)
        ),
        "got {res:?}"
    );

    // Nothing moved: an account assigned to the anonymous principal could never
    // be reached again through the caller-gated APIs.
    assert!(matches!(
        settlement_equity(&env, user_a),
        GetAccountStateResult::Ok(_)
    ));
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

    let series_id = open_matched_position(&env, user_a, user_b, "REASSIGN");

    let equity_before = match settlement_equity(&env, user_a) {
        GetAccountStateResult::Ok(resp) => resp.total_equity_usd,
        GetAccountStateResult::Err(e) => panic!("old owner state read failed: {e:?}"),
    };
    assert!(equity_before > Nat::from(0_u64));
    assert!(get_position(&env, user_a, series_id.clone()).is_some());

    let res = reassign(&env, env.controller, "reassign_move", user_a, new_owner).unwrap();
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

/// The point of the custody sweep: after a reassignment the tokens really are
/// under the new principal, and the new owner can take them off the ledger.
#[test]
fn admin_reassign_account_moves_custody_funds() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let new_owner = fresh_owner(94);

    let deposit = 10_000_000_000_u128; // 100 ICP
    env.deposit_collateral(user_a, "ICP", Nat::from(deposit), None);
    env.pic.tick();

    let old_custody = custody_account(&env, user_a);
    let new_custody = custody_account(&env, new_owner);

    assert_eq!(ledger_balance(&env, "ICP", old_custody), deposit);
    assert_eq!(ledger_balance(&env, "ICP", new_custody), 0);

    let res = reassign(&env, env.controller, "reassign_custody", user_a, new_owner).unwrap();
    assert!(matches!(res, ReassignAccountResult::Ok), "got {res:?}");
    env.pic.tick();

    // The old subaccount is emptied and the funds land under the new principal,
    // one ledger fee lighter.
    assert_eq!(
        ledger_balance(&env, "ICP", old_custody),
        0,
        "the old custody subaccount must not be left stranded"
    );
    assert_eq!(
        ledger_balance(&env, "ICP", new_custody),
        deposit - ICP_FEE,
        "the new owner's custody subaccount must hold the moved funds"
    );

    // The new owner can actually withdraw what their subaccount holds: the
    // whole balance, less the fee the withdrawal transfer itself burns.
    let withdrawal = deposit - 2 * ICP_FEE;
    let res: WithdrawCollateralResult = env
        .clearing
        .update(
            new_owner,
            "withdraw_collateral",
            (WithdrawCollateralParams {
                amount: Nat::from(withdrawal),
                asset_id: "ICP".to_owned(),
                withdrawal_id: WithdrawalId("reassign_custody_withdrawal".to_owned()),
                domain: Some(BalanceDomain::Settlement),
            },),
        )
        .unwrap();
    assert!(matches!(res, WithdrawCollateralResult::Ok), "got {res:?}");
    env.pic.tick();

    assert_eq!(ledger_balance(&env, "ICP", new_custody), 0);
    assert_eq!(
        ledger_balance(
            &env,
            "ICP",
            Account {
                owner: new_owner,
                subaccount: None,
            }
        ),
        withdrawal,
        "the new owner received the withdrawn tokens"
    );
    assert_eq!(
        settlement_balance(&env, new_owner, "ICP"),
        deposit - withdrawal
    );
}

/// Replaying a reassignment is a no-op, and the plan is what makes it one: a
/// fresh id for an account that has already moved finds no source account.
#[test]
fn admin_reassign_account_replay_is_idempotent() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let new_owner = fresh_owner(93);

    let deposit = 10_000_000_000_u128;
    env.deposit_collateral(user_a, "ICP", Nat::from(deposit), None);
    env.pic.tick();

    let res = reassign(&env, env.controller, "reassign_replay", user_a, new_owner).unwrap();
    assert!(matches!(res, ReassignAccountResult::Ok), "got {res:?}");
    env.pic.tick();

    let new_custody = custody_account(&env, new_owner);
    let custody_after_first = ledger_balance(&env, "ICP", new_custody);
    assert_eq!(custody_after_first, deposit - ICP_FEE);

    // Same id: resumes a finished reassignment, moves nothing.
    let res = reassign(&env, env.controller, "reassign_replay", user_a, new_owner).unwrap();
    assert!(matches!(res, ReassignAccountResult::Ok), "got {res:?}");
    env.pic.tick();

    assert_eq!(
        ledger_balance(&env, "ICP", new_custody),
        custody_after_first
    );
    assert_eq!(settlement_balance(&env, new_owner, "ICP"), deposit);

    // New id: there is no longer an account under the old principal to move.
    let res = reassign(&env, env.controller, "reassign_replay_2", user_a, new_owner).unwrap();
    assert!(
        matches!(
            res,
            ReassignAccountResult::Err(ReassignAccountError::AccountNotFound)
        ),
        "got {res:?}"
    );
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

    let res = reassign(
        &env,
        env.controller,
        "reassign_orders",
        user_a,
        fresh_owner(92),
    )
    .unwrap();
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
fn admin_reassign_account_rejects_frozen_position_transfers() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let user_b = test_user(55);
    let new_owner = fresh_owner(95);

    let deposit = 10_000_000_000_u128;
    env.deposit_collateral(user_a, "ICP", Nat::from(deposit), None);
    env.deposit_collateral(user_b, "ICP", Nat::from(deposit), None);
    env.pic.tick();

    let series_id = open_matched_position(&env, user_a, user_b, "REASSIGN-FROZEN");

    let proof: Option<PositionProof> = env
        .clearing
        .update(
            env.controller,
            "freeze_position_for_transfer",
            (FreezePositionForTransferParams {
                transfer_id: TransferId::from("reassign_frozen".to_owned()),
                user: User(user_a),
                series_id,
                outcome_id: None,
                valuation_price: None,
            },),
        )
        .unwrap();
    assert!(proof.is_some(), "the position must be frozen for the test");

    let balance_before = settlement_balance(&env, user_a, "ICP");
    let old_custody = custody_account(&env, user_a);
    let new_custody = custody_account(&env, new_owner);

    let res = reassign(&env, env.controller, "reassign_frozen", user_a, new_owner).unwrap();
    assert!(
        matches!(
            res,
            ReassignAccountResult::Err(ReassignAccountError::PendingPositionTransfersExist)
        ),
        "got {res:?}"
    );

    // Both sides are exactly as they were: the proof is bound to the old
    // principal, so neither the maps nor the custody funds may move.
    assert_eq!(settlement_balance(&env, user_a, "ICP"), balance_before);
    assert_eq!(ledger_balance(&env, "ICP", old_custody), deposit);
    assert_eq!(ledger_balance(&env, "ICP", new_custody), 0);
    match settlement_equity(&env, new_owner) {
        GetAccountStateResult::Err(AccountStateError::NoAccountStateFound) => {}
        other => panic!("expected NoAccountStateFound for the new owner, got {other:?}"),
    }
}

#[test]
fn admin_reassign_account_rejects_occupied_target() {
    let env = TestSetup::with_icp();
    let user_a = test_user(57);
    let user_b = test_user(58);

    env.deposit_collateral(user_a, "ICP", Nat::from(10_000_000_000_u128), None);
    env.deposit_collateral(user_b, "ICP", Nat::from(10_000_000_000_u128), None);
    env.pic.tick();

    let res = reassign(&env, env.controller, "reassign_occupied", user_a, user_b).unwrap();
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

/// The recovery path: a custody sweep that cannot settle leaves the
/// reassignment pending, both principals frozen, and the funds recoverable by
/// replaying the same id.
#[test]
fn admin_reassign_account_resumes_after_a_failed_custody_sweep() {
    let env = TestSetup::with_icp();
    let user_a = test_user(54);
    let new_owner = fresh_owner(96);

    let deposit = 10_000_000_000_u128;
    env.deposit_collateral(user_a, "ICP", Nat::from(deposit), None);
    env.pic.tick();

    let series_id = env.add_binary_series("REASSIGN-RESUME", 1_000_000, BalanceDomain::Settlement);
    let icp_ledger = env
        .ledgers
        .get("ICP")
        .expect("ledger not found")
        .canister_id();

    // A ledger that cannot answer stands in for any failure of the sweep's
    // inter-canister calls.
    env.pic
        .stop_canister(icp_ledger, Some(env.controller))
        .expect("stopping the ICP ledger failed");

    let res = reassign(&env, env.controller, "reassign_resume", user_a, new_owner).unwrap();
    assert!(
        matches!(
            res,
            ReassignAccountResult::Err(ReassignAccountError::CustodyTransferFailed { .. })
        ),
        "got {res:?}"
    );

    // The re-key stands, and both principals stay frozen while the custody
    // funds are still sitting under the old principal.
    let res = env.submit_limit_order(
        new_owner,
        "reassign_resume_order",
        series_id.clone(),
        Side::Buy,
        1,
        500_000,
    );
    assert!(
        matches!(
            res,
            SubmitMatchedTradeResult::Err(TradeError::AccountUnderReassignment { .. })
        ),
        "got {res:?}"
    );

    // A deposit from the old principal is turned away too: it would land in the
    // subaccount the sweep is about to drain.
    let res: DepositCollateralResult = env
        .clearing
        .update(
            user_a,
            "deposit_collateral",
            (DepositCollateralParams {
                amount: Nat::from(100_000_000_u128),
                asset_id: "ICP".to_owned(),
                deposit_id: DepositId("reassign_resume_deposit".to_owned()),
                domain: Some(BalanceDomain::Settlement),
            },),
        )
        .unwrap();
    assert!(
        matches!(
            res,
            DepositCollateralResult::Err(DepositCollateralError::AccountUnderReassignment { .. })
        ),
        "got {res:?}"
    );

    env.pic
        .start_canister(icp_ledger, Some(env.controller))
        .expect("starting the ICP ledger failed");
    env.pic.tick();

    assert_eq!(
        ledger_balance(&env, "ICP", custody_account(&env, user_a)),
        deposit,
        "the funds are still recoverable from the old subaccount"
    );

    // Replaying the same id picks the plan up and finishes the move.
    let res = reassign(&env, env.controller, "reassign_resume", user_a, new_owner).unwrap();
    assert!(matches!(res, ReassignAccountResult::Ok), "got {res:?}");
    env.pic.tick();

    assert_eq!(
        ledger_balance(&env, "ICP", custody_account(&env, user_a)),
        0
    );
    assert_eq!(
        ledger_balance(&env, "ICP", custody_account(&env, new_owner)),
        deposit - ICP_FEE
    );
    assert_eq!(settlement_balance(&env, new_owner, "ICP"), deposit);

    // With the reassignment finalised the new owner is free to trade again.
    let res = env.submit_limit_order(
        new_owner,
        "reassign_resume_order",
        series_id,
        Side::Buy,
        1,
        500_000,
    );
    assert!(
        matches!(res, SubmitMatchedTradeResult::Ok(_)),
        "got {res:?}"
    );
}
