use candid::Principal;
use clearing::{
    api::account::{
        params::{AggregateLeanParams, GetPositionParams},
        results::{AggregateLean, GetAccountStateResult},
    },
    types::{event::Event, margin::Position},
};

use crate::utils::{
    assertions::{assert_ok_value, assert_unauthorized},
    pic_canister::PicCanisterTrait as _,
    test_environment::{test_user, TestSetup},
};

#[test]
fn get_account_state_query_no_account() {
    let env = TestSetup::default();
    let user = test_user(10);

    let result: GetAccountStateResult = assert_ok_value(
        env.clearing
            .query::<GetAccountStateResult, _>(user, "get_account_state_query", ()),
    );

    match result {
        GetAccountStateResult::Err(e) => {
            println!("Got expected error: {e:?}");
        }
        GetAccountStateResult::Ok(_) => panic!("Expected error, got Ok"),
    }
}

#[test]
fn get_account_state_query_rejects_anonymous() {
    let env = TestSetup::default();

    let result = env.clearing.query::<GetAccountStateResult, _>(
        Principal::anonymous(),
        "get_account_state_query",
        (),
    );

    assert_unauthorized(&result);
}

#[test]
fn get_positions_empty() {
    let env = TestSetup::default();
    let user = test_user(20);

    let positions: Vec<Position> = env
        .clearing
        .query::<Vec<Position>, _>(user, "get_positions", ())
        .expect("get_positions failed");

    assert!(positions.is_empty(), "New user should have no positions");
}

#[test]
fn get_position_returns_none() {
    let env = TestSetup::default();
    let user = test_user(30);

    let params = GetPositionParams {
        series_id: "nonexistent_series".to_owned().into(),
        outcome_id: None,
    };

    let position: Option<Position> = env
        .clearing
        .query::<Option<Position>, _>(user, "get_position", (params,))
        .unwrap();

    assert!(
        position.is_none(),
        "Position for nonexistent series should be None"
    );
}

#[test]
fn get_trade_history_empty() {
    let env = TestSetup::default();
    let user = test_user(40);

    let history: Vec<Event> = env
        .clearing
        .query::<Vec<Event>, _>(user, "get_trade_history", ())
        .expect("get_trade_history failed");

    assert!(history.is_empty(), "New user should have no trade history");
}

#[test]
fn aggregate_lean_empty_for_fresh_state() {
    let env = TestSetup::default();
    let user = test_user(50);

    let params = AggregateLeanParams {
        series_id: "some_series".to_owned().into(),
        principals: vec![test_user(51), test_user(52)],
    };

    let lean: AggregateLean = env
        .clearing
        .query::<AggregateLean, _>(user, "aggregate_lean", (params,))
        .expect("aggregate_lean failed");

    assert_eq!(lean.series_id.as_str(), "some_series");
    assert_eq!(lean.total, 0);
    assert!(
        lean.outcomes.is_empty(),
        "No supplied principal holds a position yet"
    );
}

#[test]
fn aggregate_lean_rejects_anonymous() {
    let env = TestSetup::default();

    let params = AggregateLeanParams {
        series_id: "some_series".to_owned().into(),
        principals: vec![],
    };

    let result =
        env.clearing
            .query::<AggregateLean, _>(Principal::anonymous(), "aggregate_lean", (params,));

    assert_unauthorized(&result);
}
