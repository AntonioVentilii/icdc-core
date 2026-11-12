use candid::Principal;
use shared::types::{BalanceDomain, OutcomeId, Series, SeriesId, SettlementInput};

use crate::{
    api::settlement::api::{apply_settlement_accounting_logic, prepare_settlement_impl},
    memory::{ACCOUNT_STATES, POSITIONS},
    trade::{service::execute_trade_impl, types::ExecuteTradeParams},
    types::{margin::AccountState, user::User},
};

pub(crate) fn setup_test_state(users_with_balances: Vec<(User, u128)>) {
    ACCOUNT_STATES.with(|acc| {
        let mut acc = acc.borrow_mut();
        acc.clear();
        for (user, balance) in users_with_balances {
            let mut a = AccountState::new(user);
            a.set_cash_balance_usd(BalanceDomain::Settlement, balance.cast_signed());
            acc.insert(user, a);
        }
    });
    POSITIONS.with(|p| p.borrow_mut().clear());
}

pub(crate) fn execute_trade_checked(series: &Series, params: ExecuteTradeParams) {
    execute_trade_impl(series, params).expect("Trade execution failed");
}

pub(crate) fn settle_series_checked(series: &Series, input: &SettlementInput) {
    let mut plan = prepare_settlement_impl(series, &series.series_id, input, 0, 0)
        .expect("Settlement preparation failed");

    apply_settlement_accounting_logic(&mut plan);
}

pub(crate) fn verify_cash_balance(user: User, expected_usd: u128) {
    ACCOUNT_STATES.with(|acc| {
        let acc = acc.borrow();
        let actual = acc
            .get(&user)
            .expect("User account not found")
            .get_cash_balance_usd(BalanceDomain::Settlement);
        assert_eq!(
            actual,
            expected_usd.cast_signed(),
            "Cash balance mismatch for user {user:?}. Expected {expected_usd}, got {actual}"
        );
    });
}

pub(crate) fn verify_reserved_margin(user: User, expected_usd: u128) {
    ACCOUNT_STATES.with(|acc| {
        let acc = acc.borrow();
        let actual = acc
            .get(&user)
            .expect("User account not found")
            .get_reserved_margin_usd(BalanceDomain::Settlement);
        assert_eq!(
            actual, expected_usd,
            "Reserved margin mismatch for user {user:?}. Expected {expected_usd}, got {actual}"
        );
    });
}

pub(crate) fn verify_position_qty(
    user: User,
    series_id: &SeriesId,
    outcome_id: Option<&OutcomeId>,
    expected_qty: i128,
) {
    POSITIONS.with(|p| {
        let p = p.borrow();
        let actual = p
            .get(&(user, series_id.clone(), outcome_id.cloned()))
            .map_or(0, |pos| pos.net_qty);
        assert_eq!(
            actual, expected_qty,
            "Position quantity mismatch for user {user:?} in series {series_id:?}. Expected {expected_qty}, got {actual}"
        );
    });
}

pub(crate) fn create_test_user(id: u8) -> User {
    User(Principal::from_slice(&[id]))
}
