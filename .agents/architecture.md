# ICDC Architecture & Sharding

This document outlines the architectural boundaries and the sharding strategy for the ICDC system.

## 1. System Components

### Registry Canister

- **Responsibility**: Canonical definitions for products and series.
- **Ownership**: Product metadata, payoff types, oracle sources, expiry rules.
- **Excluded**: No balance or position logic.

### Clearing Canister (Economic Core)

- **Responsibility**: Central source of truth for all value.
- **Ownership**: User balances (across domains), positions, margin calculations, equity truth, settlement logic, withdrawals.
- **Domain Isolation**: Supports multiple domains (e.g., `Settlement`, `Playground`) to segregate funds and positions.

### Execution Layer (Venue/Exchange)

- **Responsibility**: Order matching and market microstructure.
- **Ownership**: Order books, match matching, fill generation.
- **Constraint**: Must verify margin with Clearing before accepting orders.

## 2. Sharding Strategy

To ensure scalability, the clearing system uses a deterministic sharding rule.

- **Sharding Rule**: `(underlying_id, expiry_month)`
  - `underlying_id`: Integer representation of the asset pair (e.g., BTC_USD).
  - `expiry_month`: `YYYYMM` (e.g., 202606).
- **Routing**: A Cleaning Router canister maps the `shard_key` to a specific `clearing_canister_id`.
- **Benefits**: Natural clustering for netting, easy lifecycle management (settling/archiving old expiries), and linear growth.

## 3. Storage & History

- **Stable Memory**: All critical state (balances, positions, config) must be stored in stable memory to survive upgrades. Use the `ic-stable-structures` crate.
- **TradeLog Sharding**: History should be sharded by time (e.g., monthly) or by clearing shard key to prevent unbounded growth of individual logs.
