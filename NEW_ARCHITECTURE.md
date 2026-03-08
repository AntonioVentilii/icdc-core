# Generic Clearing Architecture & Implementation Spec

## Goal

Define a generic multi-product clearing system that can be used by first-party and third-party venues. Vici is one consumer of this clearing, not the clearing itself.

The clearing must:

- support multiple product types
- support multi-asset collateral
- avoid swaps, bridges, and treasury management
- settle economically in a single accounting unit
- remain venue-neutral
- be implementable incrementally

---

## Core Design Principles

### 1. Separate instruments from clearing

The instrument definition must not embed chain-specific settlement rails.

A product should define:

- what it is
- how it pays
- when it expires/resolves
- which oracle determines settlement

The clearing should define:

- which collateral assets are accepted
- how collateral is valued
- how margin is computed
- how withdrawals are paid

### 2. Multi-asset collateral, single accounting unit

Users may deposit many collateral assets, but the clearing must value them in one canonical accounting unit.

Recommended accounting unit:

- `USD`

This accounting unit is used for:

- prices
- strikes
- premiums
- realised PnL
- unrealised PnL
- margin checks
- withdrawals valuation

### 3. No swaps inside the clearing

The clearing does not:

- swap assets
- bridge assets
- promise a specific payout asset beyond what exists in the vault

It only:

- holds assets
- values them
- updates accounting
- transfers assets on deposit, withdrawal, and liquidation

### 4. Vault-backed accounting

The clearing holds real collateral assets in custody.

Internal accounting determines each account's net claim on the vault.

The vault is the economic backing. The ledger allocates ownership of that vault.

---

## High-Level Architecture

### 1. Registry canister

Responsible for product and series definitions.

It owns:

- series creation
- product metadata
- expiry and resolution rules
- oracle source identifiers
- payoff configuration
- listing-independent product parameters

It does not own:

- collateral balances
- positions
- cash balances
- settlement state

### 2. Execution layer

Responsible for market microstructure.

It owns:

- order books
- active limit orders
- market orders
- matching
- cancellation
- fill generation

It does not own:

- collateral
- margin truth
- final settlement truth

This layer should be shardable.

### 3. Clearing layer

Responsible for all economic truth.

It owns:

- collateral inventory
- collateral valuation
- account equity
- reserved margin
- positions
- accepted fills
- settlement
- withdrawals
- liquidations

This layer should remain central for account truth unless there is a compelling reason to shard it later.

### 4. Venue / application layer

Vici is one venue on top of the infrastructure.

A venue may:

- display products
- display depth and market data
- route orders
- attract liquidity
- provide social and UX layers

A venue does not own settlement truth.

---

## Product Model

### Series must define economics, not token rails

The current design should be changed from per-series `settlement_asset` to a canonical payout / quote unit.

Recommended change:

- remove `settlement_asset` from `Series`
- replace with `payout_unit` or `quote_unit`

Recommended enum for now:

```rust
pub enum PayoutUnit {
    USD,
}
```

This allows the same series definition to be used across different deployments or withdrawal rails later.

### Recommended `Series` model

```rust
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct Series {
    pub series_id: SeriesId,
    pub underlying: String,
    pub expiry_ns: u64,
    pub payoff_type: PayoffType,
    pub strike: Option<Price>,
    pub price_precision: u8,
    pub payout_unit: PayoutUnit,
    pub oracle_source: String,
    pub creator: Principal,
    pub created_at_ns: u64,
    pub title: String,
    pub description: Description,
}
```

### Product support target

The clearing should be generic enough to support:

- binary markets
- scalar markets
- multi-outcome markets
- calls
- puts
- digital / capped options

The generic principle is:

```text
payout = quantity × payoff_function(outcome)
```

where the payoff is expressed in the product's `payout_unit`.

```rust
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PayoutUnit {
    Fiat(FiatUnit),
    Crypto(CanonicalCryptoUnit),
    NonMonetary(NonMonetaryUnit),
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FiatUnit {
    Usd,
    Eur,
    Gbp,
    Chf,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CanonicalCryptoUnit {
    Btc,
    Eth,
    Icp,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NonMonetaryUnit {
    Points,
}
```

---

## Collateral Model

### Supported collateral assets

The clearing may support multiple collateral assets across multiple chains, such as:

- ICRC assets
- ERC-20 assets
- native assets

Each supported collateral asset must have a static risk configuration.

### Collateral asset configuration

Each asset should define at least:

- asset id
- chain / standard information
- decimals
- static price source input
- static haircut
- enabled / disabled flag

Recommended model:

```rust
pub struct CollateralAssetConfig {
    pub asset_id: AssetId,
    pub symbol: String,
    pub decimals: u8,
    pub price_usd: Decimal,
    pub haircut_bps: u16,
    pub is_enabled: bool,
}
```

### Collateral value

For now, collateral valuation can be static and conservative.

Formula:

```text
collateral_value_usd = amount × price_usd × haircut
```

Haircuts are static for phase two.

Example:

- USDC: 100%
- BTC-like: 80–90%
- ETH-like: 75–85%
- ICP-like: lower and conservative

No correlation matrix, beta model, or portfolio VaR is required initially.

---

## Custody Model

### Clearing-controlled user subaccounts

Each user's collateral should be held in a clearing-controlled subaccount.

Important distinction:

- the clearing controls the account owner
- each user is mapped to a distinct subaccount

This means collateral is already in clearing custody.

As a result:

- settlement does not require token transfers into the vault
- the funds are already inside the vault system
- settlement only changes internal accounting

Actual token transfers occur only on:

- deposit
- withdrawal
- liquidation

---

## Accounting Model

### Separate collateral inventory from cash accounting

Each account needs two distinct layers:

#### A. Collateral balances by asset

These are real deposited assets.

#### B. Internal accounting balance in USD

This represents realised PnL / debt / credits in the clearing accounting unit.

This should not be confused with a real stablecoin balance.

### Recommended account state

```rust
pub struct AccountState {
    pub collateral_balances: BTreeMap<AssetId, Amount>,
    pub cash_balance_usd: SignedAmount,
    pub reserved_margin_usd: Amount,
    pub positions: BTreeMap<SeriesId, Position>,
}
```

Derived values:

- `collateral_value_usd`
- `unrealised_pnl_usd`
- `equity_usd`
- `free_margin_usd`
- `withdrawable_usd`

### Equity formula

```text
equity_usd = collateral_value_usd + cash_balance_usd + unrealised_pnl_usd
```

This is the final net value that matters.

Users should be shown equity, not only raw cash balance.

---

## Settlement Model

### Settlement updates accounting, not physical assets

When a product expires or resolves:

- compute payoff
- close or update positions
- update realised PnL in USD accounting
- recompute margin and equity

Do not physically transfer collateral assets between users during normal settlement.

Reason:

- avoids implicit swaps
- avoids forcing winners to receive random volatile assets at resolution time
- avoids per-settlement pricing and rounding complexity
- scales much better

### Settlement formula

For each account and position:

```text
realised_cashflow_usd = quantity × payoff_function(final_outcome)
```

Then:

```text
account.cash_balance_usd += realised_cashflow_usd
```

### Important consequence

A user may have:

- positive or negative cash balance
- positive collateral inventory
- positive or negative unrealised PnL

The system should reason on equity, not on a single field in isolation.

---

## Margin & Solvency Model

### Margin checks

Before accepting orders or allowing withdrawals, the clearing must check account solvency.

Minimum phase-two model:

- initial margin check
- maintenance margin check
- reserved margin accounting

### Solvency invariant

The clearing must not allow accounts to remain indefinitely with unsustainable negative equity.

Primary invariant:

```text
equity_usd >= maintenance_margin_usd
```

If violated, the account is liquidatable.

### Liquidation

Liquidation is the mechanism that turns negative cash / losses into reductions of collateral.

The clearing does not need to do this during normal settlement.

It does it only when the account breaches maintenance requirements.

---

## Withdrawals

### The key rule

If the clearing does not perform swaps, it cannot promise withdrawal in a specific asset that is not present in the vault.

Therefore:

- withdrawals are paid using assets that actually exist in the vault
- valuation is performed in USD accounting terms
- asset allocation follows a deterministic policy

### Recommended withdrawal policies

Possible policies:

#### Option A — Pro-rata by vault composition

Most self-sustaining and neutral.

#### Option B — Stable-first waterfall

Example priority:

1. USDC-like assets
2. other approved stablecoins
3. non-stable collateral assets

#### Option C — User preference with deterministic fallback

Best UX:

- try requested asset first
- if insufficient, fall back to deterministic mixed payout

Recommended approach:

- implement stable-first waterfall now
- keep the fallback explicit and deterministic

### Withdrawal safety check

Before withdrawal:

```text
equity_after_withdrawal >= maintenance_margin_usd
```

### Full withdrawal meaning

A user requesting to withdraw everything receives:

- their net withdrawable equity value
- paid out in supported assets available in the vault
- according to the chosen allocation policy

This is the only self-sustaining no-swap model.

---

## Internal USD Ledger Token

### Recommendation: use an internal ghost ICRC token

This is a good idea, provided it is treated as internal accounting only.

Suggested concept:

- `vUSD` or `virtualUSD` or `cvUSD` or `ClearingVirtualUSD`

Purpose:

- represent internal realised PnL / cash claims
- simplify indexing and transaction history
- avoid custom cash-balance history implementation in clearing

### Rules for the ghost token

The token must:

- be minted and burned only by the clearing
- not be marketed as a redeemable stablecoin
- represent a claim on the clearing's vault-backed accounting system

### Benefits

- native balance queries
- transaction/index support
- simpler audit trail
- cleaner separation between risk engine and accounting balances

### Withdrawal behaviour

When a user withdraws realised value:

- burn `cvUSD`
- transfer supported vault assets worth the requested amount

### Important warning

Never imply:

```text
1 cvUSD == 1 USDC redeemable on demand in USDC only
```

Correct interpretation:

- `1 cvUSD` is `1 USD` of internal accounting claim
- redemption occurs into available vault assets according to withdrawal policy

---

## Order & Execution Flow

### Recommended execution contract

The order book should not be part of the clearing core long-term.

The recommended interaction is:

1. venue submits order to execution layer
2. execution asks clearing to reserve required margin
3. if reserve succeeds, order becomes live
4. execution matches orders and generates fills
5. execution submits fills to clearing
6. clearing validates fill and updates positions / cash / margin

### Key invariant

No live order should exist unless the clearing has already reserved the economic capacity required to honour it.

### Why this matters

This allows:

- pluggable venues
- future third-party venues
- shardable order books
- central account truth in clearing

---

## What Vici Needs From Clearing

Vici only needs a minimal venue-facing interface.

### Read endpoints

- list series
- get series
- list supported collateral assets and haircuts
- get account state
- get positions
- get withdrawable value
- get balance of internal cash token

### Write endpoints

- deposit collateral
- withdraw collateral / equity
- place market order
- place limit order
- cancel order

Vici does not need to implement settlement logic.

---

## Recommended Phase Plan

### Phase 1 — already available / current base

- registry + clearing
- per-product payout model
- simple execution inside clearing if needed
- single accounting unit concept

### Phase 2 — next target

- replace per-series `settlement_asset` with `payout_unit`
- add multi-asset collateral configs
- add static prices and static haircuts
- add equity / reserved margin / withdrawal calculations
- add vault-backed withdrawal policy
- optionally add ghost ICRC token for internal USD accounting

### Phase 3 — later

- separate execution canisters from clearing
- shard order books
- add liquidation engine improvements
- add dynamic price feeds
- add concentration limits
- add richer product families

---

## Minimal Implementation Checklist

### Registry

- [ ] replace `settlement_asset` with `payout_unit`
- [ ] keep series product-definition only
- [ ] add generic payoff metadata where required

### Clearing

- [ ] add supported collateral asset config
- [ ] add static haircut support
- [ ] add static price support
- [ ] add account equity calculation
- [ ] add reserved margin tracking
- [ ] add withdrawable amount calculation
- [ ] add settlement as USD accounting updates
- [ ] add deterministic withdrawal allocation policy
- [ ] add liquidation hooks

### Optional internal ledger token

- [ ] create `cvUSD` ICRC ledger + index
- [ ] make clearing sole minter/burner
- [ ] map realised cash accounting to `cvUSD`
- [ ] burn `cvUSD` on withdrawal

### Execution

- [ ] define `reserve_margin`
- [ ] define `release_margin`
- [ ] define `submit_fill`
- [ ] plan future separation into execution shards

---

## Final Recommended Position

The clearing should be implemented as:

- a generic clearing core
- multi-asset collateral
- single accounting unit (`USD`)
- vault-backed self-sustaining accounting
- no swaps / no bridges / no liquidity management
- deterministic withdrawal policy from available vault assets
- optional internal ghost ICRC token for accounting balances

This keeps the system:

- generic
- composable
- self-sustaining
- auditable
- usable by Vici and future third-party venues
