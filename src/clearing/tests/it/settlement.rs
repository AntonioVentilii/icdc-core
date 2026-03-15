use candid::Principal;
use clearing::{
    api::settlement::{params::SettleSeriesParams, results::SettleSeriesResult},
    types::plans::{SettlementPlan, SettlementStatusView},
};
use shared::types::{Price, SettlementInput};

use crate::utils::{
    assertions::{assert_ok_value, assert_unauthorized},
    pic_canister::PicCanisterTrait as _,
    test_environment::{test_user, TestSetup},
};

#[test]
fn get_settlement_status_nonexistent() {
    let env = TestSetup::default();
    let user = test_user(60);

    let status: Option<SettlementStatusView> =
        assert_ok_value(env.clearing.query::<Option<SettlementStatusView>, _>(
            user,
            "get_settlement_status",
            ("nonexistent_series".to_owned(),),
        ));

    assert!(status.is_none(), "No settlement plan should exist");
}

#[test]
fn settle_series_unauthorized() {
    let env = TestSetup::default();

    let params = SettleSeriesParams {
        series_id: "test_series".to_owned().into(),
        settlement: SettlementInput::Price(Price::new(100_000_000, 6)),
    };

    let result = env.clearing.update::<SettleSeriesResult, _>(
        Principal::anonymous(),
        "settle_series",
        (params,),
    );

    assert_unauthorized(&result);
}

#[test]
fn get_settlement_plan_nonexistent() {
    let env = TestSetup::default();

    let plan: Option<SettlementPlan> =
        assert_ok_value(env.clearing.query::<Option<SettlementPlan>, _>(
            env.controller,
            "get_settlement_plan",
            ("nonexistent".to_owned(),),
        ));

    assert!(plan.is_none());
}

#[test]
fn resume_settlement_no_plan() {
    let env = TestSetup::default();

    let result: SettleSeriesResult = assert_ok_value(env.clearing.update::<SettleSeriesResult, _>(
        env.controller,
        "resume_settlement",
        ("nonexistent".to_owned(),),
    ));

    match result {
        SettleSeriesResult::Err(_) => {
            // Expected: no plan to resume
        }
        other => panic!("Expected error, got: {other:?}"),
    }
}
