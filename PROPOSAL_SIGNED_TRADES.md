# Proposal: Secure P2P Trading via Signed Intent

This proposal outlines how to allow users to trade directly with the Clearing Canister (P2P) without requiring an intermediate Exchange Canister, while maintaining absolute security against forged trades.

## 1. The Security Gap

Currently, `submit_matched_trade` trusts the **caller** (the Exchange). If we allow a user to call this from a frontend:

- Alice could submit a trade where she "buys" 1000 units from Bob at a price of 0.
- Without Bob's explicit consent inside the Clearing Canister, Bob's collateral would be drained.

## 2. The Solution: Dual-Signature Verification

We move from "Trusting the Caller" to "Trusting the Data". A trade is only valid if both the Buyer and Seller have cryptographically signed the **Intent** of the trade.

### 2.1 Message Format (The "Intent")

To prevent replay attacks and ensure the signature is specific to this system, both parties sign a hash of:

1. `Domain Separator`: (e.g., `\x19ICDC_TRADE_INTENT`)
2. `trade_id`: Unique ID to prevent replaying the same trade twice.
3. `series_id`: To ensure the trade is for the correct market.
4. `buyer` & `seller`: To bind the signatures to specific identities.
5. `qty` & `price`: The core terms of the contract.

### 2.2 Updated Data Model

Modify `SubmitMatchedTradeParams` in `src/clearing/src/types/params.rs`:

```rust
pub struct SubmitMatchedTradeParams {
    pub trade_id: TradeId,
    pub series_id: SeriesId,
    pub buyer: Principal,
    pub seller: Principal,
    pub qty: i128,
    pub price: u64,
    // Support for signatures
    pub buyer_signature: Option<Vec<u8>>,
    pub seller_signature: Option<Vec<u8>>,
}
```

## 3. Implementation Workflow

### Step 1: Crypto Dependencies

Add verification crates to `Cargo.toml`. While standard libraries like `ed25519-dalek` work, the IC community often uses `ic-sig-verifier` to handle the specificities of IC Principal signatures (which can be delegations).

### Step 2: Signature Helper Logic

Implement a utility in `src/clearing/src/utils/crypto.rs` to:

1. Reconstruct the message hash.
2. Verify a signature against a Principal.
   - _Note_: Since an IC Principal is essentially a hash of a public key (or a delegation), the verification process involves extracting the public key from the signature proof and checking it matches the Principal.

### Step 3: Gated Logic in `submit_matched_trade`

The function logic would be updated to:

```rust
pub async fn submit_matched_trade(params: SubmitMatchedTradeParams) -> Result<bool, TradeError> {
    let caller = ic_cdk::caller();

    // 1. Check if it's a "B2B" trusted call
    let is_authorized_exchange = AUTHORIZED_EXCHANGES.with(|a| {
        *a.borrow().get(&caller).unwrap_or(&false)
    }) || ic_cdk::api::is_controller(&caller);

    if !is_authorized_exchange {
        // 2. If it's a P2P call (from a user/frontend), signatures are MANDATORY
        let buyer_sig = params.buyer_signature.ok_or(TradeError::Unauthorized)?;
        let seller_sig = params.seller_signature.ok_or(TradeError::Unauthorized)?;

        // 3. Verify signatures match buyer and seller principals
        verify_trade_signatures(&params, &buyer_sig, &seller_sig)?;
    }

    // ... proceed with trade execution ...
}
```

## 4. Feasibility on the Internet Computer (IC)

**Is this possible? Yes, and it is a best practice.**

- **Signature Standards**: Most IC wallets (Internet Identity, Plug, NFID) use Ed25519 or Secp256k1.
- **Canister Performance**: Signature verification is computationally intensive (linear logic) but well within the instruction limits of a single update call.
- **Frontend Integration**: Using `agent-js`, the frontend can sign the trade object before submitting it to the canister.

## 5. Benefits

1. **True P2P**: Users can build their own "OTC" tools or "Rush Mode" UIs without needing to deploy a centralized exchange canister.
2. **Hybrid Security**: You can still have "Exchanges" for fast matching, while allowing P2P "fallback" for maximum decentralization.
3. **Non-Custodial Integrity**: The Clearing Canister acts as a true immutable judge that only moves assets when two valid signatures are presented.
