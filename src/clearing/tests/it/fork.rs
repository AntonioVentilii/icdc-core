use candid::{decode_one, encode_one};
use shared::types::{
    engine::EngineRole,
    groups::GroupId,
    series::{AddSeriesResult, ForkSeriesParams, SeriesError},
    BalanceDomain, Series, SeriesId, TradingAccess,
};

use crate::utils::{
    pic_canister::PicCanisterTrait as _,
    test_environment::{test_user, TestSetup},
    trade_helper::TradeHelperTrait as _,
};

#[test]
fn fork_series_creates_distinct_restricted_series() {
    let setup = TestSetup::default();

    let source_id = setup.add_binary_series("ICP", 50_000, BalanceDomain::Settlement);
    setup.pic.tick();

    let group = GroupId::from("grp_fork_1".to_owned());
    let trading_access = vec![TradingAccess::Restricted {
        groups: vec![group],
    }];

    let fork_res = setup.fork_binary_series(setup.controller, &source_id, trading_access);

    let fork_id = match fork_res {
        AddSeriesResult::Ok(id) => id,
        AddSeriesResult::Err(e) => panic!("Fork failed: {e:?}"),
    };

    assert_ne!(source_id, fork_id, "Forked series must have a different ID");

    let forked: Option<Series> = setup
        .registry
        .query(setup.controller, "get_series", (fork_id.clone(),))
        .unwrap();

    let forked = forked.expect("Forked series should exist");
    assert_eq!(forked.forked_from, Some(source_id));
    assert!(
        matches!(&forked.trading_access[0], TradingAccess::Restricted { .. }),
        "Forked series must be restricted"
    );
}

#[test]
fn engine_creator_can_fork() {
    let setup = TestSetup::default();
    let creator = test_user(70);

    let engine_id = setup.register_engine("Fork Engine", vec![EngineRole::Creator]);
    setup.grant_engine_role(&engine_id, creator, EngineRole::Creator);
    setup.pic.tick();

    let source_id = setup.add_binary_series("BTC", 100_000, BalanceDomain::Settlement);
    setup.pic.tick();

    let group = GroupId::from("grp_fork_2".to_owned());
    let trading_access = vec![TradingAccess::Restricted {
        groups: vec![group],
    }];

    let fork_res = setup.fork_binary_series(creator, &source_id, trading_access);
    assert!(
        matches!(fork_res, AddSeriesResult::Ok(_)),
        "Engine creator should be able to fork"
    );
}

#[test]
fn non_creator_cannot_fork() {
    let setup = TestSetup::default();
    let random_user = test_user(71);

    let source_id = setup.add_binary_series("ICP", 50_000, BalanceDomain::Settlement);
    setup.pic.tick();

    let group = GroupId::from("grp_fork_3".to_owned());
    let trading_access = vec![TradingAccess::Restricted {
        groups: vec![group],
    }];

    let fork_res = setup.fork_binary_series(random_user, &source_id, trading_access);
    assert!(
        matches!(fork_res, AddSeriesResult::Err(SeriesError::Unauthorized)),
        "Non-creator should be rejected"
    );
}

#[test]
fn fork_must_be_restricted() {
    let setup = TestSetup::default();

    let source_id = setup.add_binary_series("ICP", 50_000, BalanceDomain::Settlement);
    setup.pic.tick();

    let fork_params = ForkSeriesParams {
        source_series_id: source_id,
        title: None,
        description: None,
        trading_access: vec![TradingAccess::Open],
    };

    let res_bytes = setup
        .pic
        .update_call(
            setup.registry.canister_id(),
            setup.controller,
            "fork_series",
            encode_one(fork_params).unwrap(),
        )
        .expect("fork_series call failed");

    let res: AddSeriesResult = decode_one(&res_bytes).unwrap();
    assert!(
        matches!(res, AddSeriesResult::Err(SeriesError::ForkMustBeRestricted)),
        "Fork with Open access should be rejected"
    );
}

#[test]
fn fork_nonexistent_source() {
    let setup = TestSetup::default();

    let fork_params = ForkSeriesParams {
        source_series_id: SeriesId::from("nonexistent".to_owned()),
        title: None,
        description: None,
        trading_access: vec![TradingAccess::Restricted {
            groups: vec![GroupId::from("grp_1".to_owned())],
        }],
    };

    let res_bytes = setup
        .pic
        .update_call(
            setup.registry.canister_id(),
            setup.controller,
            "fork_series",
            encode_one(fork_params).unwrap(),
        )
        .expect("fork_series call failed");

    let res: AddSeriesResult = decode_one(&res_bytes).unwrap();
    assert!(
        matches!(res, AddSeriesResult::Err(SeriesError::SourceSeriesNotFound)),
        "Fork of nonexistent source should fail"
    );
}
