# SHARDING STRATEGY: ICDC Clearing

This document outlines the architectural strategy for sharding the Clearing layer of ICDC. As the platform grows, a single clearing canister will eventually hit memory and compute limits. Sharding ensures linear scalability.

## Core Principle: Deterministic Bucket Routing

Instead of random or round-robin sharding, ICDC uses **instrument-bucket sharding** based on the underlying asset and the expiry of the derivative.

### The Shard Key

Every clearing operation (trade submission, position lookup, settlement) derives a `shard_key`:

```text
shard_key = (underlying_id, expiry_month)
```

- **`underlying_id`**: A canonical `u32` assigned by the **Registry** (e.g., `BTC = 1`, `ETH = 2`).
- **`expiry_month`**: A `u32` representing the year and month of expiry in `YYYYMM` format (e.g., `202606` for June 2026).

## Components

### 1. Registry (Source of Truth)

The Registry manages the mapping of asset tickers (like "BTC/USD") to canonical `underlying_id`s. This prevents reliance on strings in performance-critical routing logic.

### 2. Clearing Router (The Traffic Controller)

The Router is the first point of contact for external participants (Exchanges, OTC desks).

- **Function**: `resolve_clearing(underlying_id: u32, expiry_month: u32) -> Principal`
- **Logic**: It maintains a mapping of `(shard_key) -> clearing_canister_id`. If no shard exists for a requested key, the Router can trigger the deployment of a new shard or assign it to an existing "active" canister.

### 3. Clearing Shards (The Workers)

Each Clearing Canister instance handles one or more buckets.

- **State Scoping**: A shard only stores positions and margin accounts for series belonging to its assigned `shard_key`.
- **Settlement Isolation**: Settlement processes for "ETH June 2026" never block or interfere with "BTC December 2026".

## Workflow: Submitting a Trade

1. **Exchange** queries **Registry** for the `underlying_id` of the asset.
2. **Exchange** calculates `expiry_month` from the contract's expiry timestamp.
3. **Exchange** calls **Router**.`resolve_clearing(u_id, exp_m)` to get the target canister.
4. **Exchange** calls **ClearingShard**.`submit_matched_trade(...)`.

## Why This Strategy?

1. **Natural Clustering**: Options and futures naturally cluster around expiries. Netting and margin offsets are most common within the same expiry month.
2. **Deterministic Routing**: Any participant can calculate the shard key and locate the correct canister without complex lookups.
3. **Easy Archiving**: Once an `expiry_month` has passed and all series are settled, the entire shard (or its data) can be moved to "cold" storage or reduced to read-only status.
4. **Performance Isolation**: A "hot" underlying (e.g., a massive ETH volatility event) only impacts the shards handling ETH for that specific month.

## Future Scaling: Sub-Sharding

If a single `(underlying_id, expiry_month)` becomes too large (rare but possible for extremely high-volume assets), the strategy can be extended to `(underlying_id, expiry_week)` or `(underlying_id, expiry_month, user_prefix_hash)`.
