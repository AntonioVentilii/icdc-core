use std::collections::BTreeMap;

use ic_cdk::api::time;
use ic_cdk_macros::update;

// use crate::types::user::User;
use crate::types::margin::{MarginAccount, Position};
// use shared::types::SeriesId;
use crate::{
    guards::{caller_is_controller, caller_is_not_anonymous},
    memory::{EVENTS, MARGIN_ACCOUNTS, NEXT_EVENT_ID, POSITIONS},
    types::error::ClearingError,
};
use crate::{
    types::{
        event::{Event, EventType},
        params::{FreezePositionForTransferParams, SubmitMatchedTradeParams},
        results::{AcceptPositionTransferResult, SubmitMatchedTradeResult},
        state::PositionProof,
    },
    utils::series::ensure_series_registered,
};

#[update(guard = "caller_is_not_anonymous")]
pub async fn submit_matched_trade(params: SubmitMatchedTradeParams) -> SubmitMatchedTradeResult {
    let result: Result<bool, ClearingError> = (async {
        let SubmitMatchedTradeParams {
            series_id,
            buyer,
            seller,
            qty,
            price,
        } = params;

        let series = ensure_series_registered(&series_id).await?;

        let settlement_asset = series.settlement_asset.to_asset();

        let required_margin = qty.unsigned_abs() * (price as u128) / 1000000;

        MARGIN_ACCOUNTS.with(|accounts| {
            let mut accounts = accounts.borrow_mut();

            let buyer_account = accounts.entry(buyer).or_insert(MarginAccount {
                user: buyer,
                balances: BTreeMap::new(),
                required_margin: 0,
            });

            let buyer_collateral = buyer_account.get_balance(&settlement_asset);

            let new_buyer_required = buyer_account.required_margin + required_margin;
            if new_buyer_required > buyer_collateral {
                return Err(ClearingError::BuyerInsufficientMargin);
            }
            buyer_account.required_margin = new_buyer_required;

            let seller_account = accounts.entry(seller).or_insert(MarginAccount {
                user: seller,
                balances: BTreeMap::new(),
                required_margin: 0,
            });

            let seller_collateral = seller_account.get_balance(&settlement_asset);

            let new_seller_required = seller_account.required_margin + required_margin;
            if new_seller_required > seller_collateral {
                return Err(ClearingError::SellerInsufficientMargin);
            }
            seller_account.required_margin = new_seller_required;
            Ok(())
        })?;

        POSITIONS.with(|positions| {
            let mut positions = positions.borrow_mut();

            let buyer_pos = positions
                .entry((buyer, series_id.clone()))
                .or_insert(Position {
                    user: buyer,
                    series_id: series_id.clone(),
                    net_qty: 0,
                });
            buyer_pos.net_qty += qty;

            let seller_pos = positions
                .entry((seller, series_id.clone()))
                .or_insert(Position {
                    user: seller,
                    series_id: series_id.clone(),
                    net_qty: 0,
                });
            seller_pos.net_qty -= qty;
        });

        let event_id = NEXT_EVENT_ID.with(|id| {
            let mut id = id.borrow_mut();
            let current = *id;
            *id += 1;
            current
        });

        EVENTS.with(|events| {
            events.borrow_mut().push(Event {
                event_id,
                clearing_id: ic_cdk::id(),
                series_id,
                user: buyer,
                qty,
                price,
                event_type: EventType::Executed,
                timestamp: time(),
            });
        });

        Ok(true)
    })
    .await;

    result.into()
}

#[update(guard = "caller_is_controller")] // Needs caller_is_controller which I'll import
pub fn freeze_position_for_transfer(
    params: FreezePositionForTransferParams,
) -> Option<PositionProof> {
    let FreezePositionForTransferParams { user, series_id } = params;

    POSITIONS.with(|positions| {
        let mut positions = positions.borrow_mut();
        if let Some(pos) = positions.remove(&(user, series_id.clone())) {
            Some(PositionProof {
                user,
                series_id,
                qty: pos.net_qty,
                clearing_id: ic_cdk::id(),
                signature: vec![],
            })
        } else {
            None
        }
    })
}

#[update(guard = "caller_is_controller")]
pub async fn accept_position_transfer(proof: PositionProof) -> AcceptPositionTransferResult {
    let result: Result<bool, ClearingError> = (async {
        ensure_series_registered(&proof.series_id).await?;

        POSITIONS.with(|positions| {
            let mut positions = positions.borrow_mut();
            let pos = positions
                .entry((proof.user, proof.series_id.clone()))
                .or_insert(Position {
                    user: proof.user,
                    series_id: proof.series_id,
                    net_qty: 0,
                });
            pos.net_qty += proof.qty;
        });

        Ok(true)
    })
    .await;

    result.into()
}
