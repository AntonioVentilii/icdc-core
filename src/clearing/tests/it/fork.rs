use candid::{decode_one, encode_one};
use shared::types::{
    engine::EngineRole,
    groups::GroupId,
    series::{AddSeriesResult, ForkSeriesParams, SeriesError},
    BalanceDomain, Description, Series, SeriesId, TradingAccess,
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

    let fork_res = setup.fork_binary_series(setup.controller, &source_id, trading_access, None);

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

    let fork_res = setup.fork_binary_series(creator, &source_id, trading_access, Some(engine_id));
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

    let fork_res = setup.fork_binary_series(random_user, &source_id, trading_access, None);
    assert!(
        matches!(
            fork_res,
            AddSeriesResult::Err(SeriesError::EngineIdRequired)
        ),
        "Non-controller without engine_id should be rejected"
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
        engine_id: None,
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
        engine_id: None,
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

#[test]
fn multiple_forks_from_same_source_produce_unique_ids() {
    let setup = TestSetup::default();

    let source_id = setup.add_binary_series("ICP", 50_000, BalanceDomain::Settlement);
    setup.pic.tick();

    let group = GroupId::from("grp_multi".to_owned());
    let access = || {
        vec![TradingAccess::Restricted {
            groups: vec![group.clone()],
        }]
    };

    let fork1 = setup.fork_binary_series(setup.controller, &source_id, access(), None);
    let AddSeriesResult::Ok(fork1_id) = fork1 else {
        panic!("Fork 1 failed");
    };
    setup.pic.tick();

    let fork2 = setup.fork_binary_series(setup.controller, &source_id, access(), None);
    let AddSeriesResult::Ok(fork2_id) = fork2 else {
        panic!("Fork 2 failed");
    };

    assert_ne!(fork1_id, fork2_id, "Multiple forks must produce unique IDs");
    assert_ne!(fork1_id, source_id);
    assert_ne!(fork2_id, source_id);
}

#[test]
fn fork_empty_trading_access_rejected() {
    let setup = TestSetup::default();

    let source_id = setup.add_binary_series("ICP", 50_000, BalanceDomain::Settlement);
    setup.pic.tick();

    let fork_params = ForkSeriesParams {
        source_series_id: source_id,
        title: None,
        description: None,
        trading_access: vec![],
        engine_id: None,
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
        "Empty trading_access should be rejected for forks"
    );
}

#[test]
fn fork_mixed_open_and_restricted_rejected() {
    let setup = TestSetup::default();

    let source_id = setup.add_binary_series("ICP", 50_000, BalanceDomain::Settlement);
    setup.pic.tick();

    let fork_params = ForkSeriesParams {
        source_series_id: source_id,
        title: None,
        description: None,
        engine_id: None,
        trading_access: vec![
            TradingAccess::Open,
            TradingAccess::Restricted {
                groups: vec![GroupId::from("grp_mixed".to_owned())],
            },
        ],
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
        "Mixed Open+Restricted should be rejected for forks"
    );
}

#[test]
fn fork_title_too_long_rejected() {
    let setup = TestSetup::default();

    let source_id = setup.add_binary_series("ICP", 50_000, BalanceDomain::Settlement);
    setup.pic.tick();

    let fork_params = ForkSeriesParams {
        source_series_id: source_id,
        title: Some("x".repeat(129)),
        description: None,
        trading_access: vec![TradingAccess::Restricted {
            groups: vec![GroupId::from("grp_title".to_owned())],
        }],
        engine_id: None,
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
        matches!(res, AddSeriesResult::Err(SeriesError::TitleTooLong)),
        "Fork with title > 128 chars should be rejected"
    );
}

#[test]
fn fork_description_too_long_rejected() {
    let setup = TestSetup::default();

    let source_id = setup.add_binary_series("ICP", 50_000, BalanceDomain::Settlement);
    setup.pic.tick();

    let fork_params = ForkSeriesParams {
        source_series_id: source_id,
        title: None,
        description: Some(Description::plain("y".repeat(1025))),
        trading_access: vec![TradingAccess::Restricted {
            groups: vec![GroupId::from("grp_desc".to_owned())],
        }],
        engine_id: None,
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
        matches!(res, AddSeriesResult::Err(SeriesError::DescriptionTooLong)),
        "Fork with description > 1024 chars should be rejected"
    );
}

#[test]
fn fork_with_engine_id_propagates_to_series() {
    let setup = TestSetup::default();
    let creator = test_user(80);

    let engine_id = setup.register_engine("Fork EID Engine", vec![EngineRole::Creator]);
    setup.grant_engine_role(&engine_id, creator, EngineRole::Creator);
    setup.pic.tick();

    let source_id = setup.add_binary_series("ICP", 50_000, BalanceDomain::Settlement);
    setup.pic.tick();

    let fork_res = setup.fork_binary_series(
        creator,
        &source_id,
        vec![TradingAccess::Restricted {
            groups: vec![GroupId::from("grp_eid".to_owned())],
        }],
        Some(engine_id.clone()),
    );
    let AddSeriesResult::Ok(fork_id) = fork_res else {
        panic!("Fork with engine_id should succeed");
    };

    let forked: Option<Series> = setup
        .registry
        .query(setup.controller, "get_series", (fork_id,))
        .unwrap();

    let forked = forked.expect("Forked series should exist");
    assert_eq!(
        forked.engine_id,
        Some(engine_id),
        "Forked series should carry the engine_id"
    );
}

#[test]
fn fork_with_wrong_engine_id_rejected() {
    let setup = TestSetup::default();
    let creator = test_user(81);

    let engine_a = setup.register_engine("Fork Eng A", vec![EngineRole::Creator]);
    let engine_b = setup.register_engine("Fork Eng B", vec![EngineRole::Creator]);
    setup.grant_engine_role(&engine_a, creator, EngineRole::Creator);
    setup.pic.tick();

    let source_id = setup.add_binary_series("ICP", 50_000, BalanceDomain::Settlement);
    setup.pic.tick();

    let fork_res = setup.fork_binary_series(
        creator,
        &source_id,
        vec![TradingAccess::Restricted {
            groups: vec![GroupId::from("grp_wrong".to_owned())],
        }],
        Some(engine_b),
    );
    assert!(
        matches!(
            fork_res,
            AddSeriesResult::Err(SeriesError::EngineRoleNotHeld)
        ),
        "Fork with wrong engine_id should be rejected"
    );
}

#[test]
fn fork_non_controller_without_engine_id_rejected() {
    let setup = TestSetup::default();
    let creator = test_user(82);

    let engine_id = setup.register_engine("Fork No EID", vec![EngineRole::Creator]);
    setup.grant_engine_role(&engine_id, creator, EngineRole::Creator);
    setup.pic.tick();

    let source_id = setup.add_binary_series("ICP", 50_000, BalanceDomain::Settlement);
    setup.pic.tick();

    let fork_res = setup.fork_binary_series(
        creator,
        &source_id,
        vec![TradingAccess::Restricted {
            groups: vec![GroupId::from("grp_noeid".to_owned())],
        }],
        None,
    );
    assert!(
        matches!(
            fork_res,
            AddSeriesResult::Err(SeriesError::EngineIdRequired)
        ),
        "Non-controller without engine_id should be rejected"
    );
}
