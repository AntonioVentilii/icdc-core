Here’s a sharding scheme that stays sane from **digitals → vanilla options → full CCP**, without painful migrations.

## Goal

Keep each clearing canister’s **live state bounded** by:

- number of active series it serves
- number of (user, series) position entries
- peak trading load

…and make routing deterministic so any exchange/OTC desk can submit trades to the right clearing automatically.

---

## Sharding rule (recommended): **(underlying_id, expiry_month)**

### 1) Canonical identifiers

- `underlying_id`: small integer (from Registry), not text
  - examples: `BTC_USD`, `ETH_USD`, `ICP_USD`, “EU CPI YoY”, etc.

- `expiry_month`: `YYYYMM` (e.g. `202606`)

### 2) Clearing shard key

```text
shard_key = (underlying_id, expiry_month)
```

### 3) Routing

A tiny **Clearing Router** canister maps:

```text
(shard_key) -> clearing_canister_id
```

If missing, it can spawn/register a new shard (or you can pre-create them).

**Why this works**

- Vanilla options naturally cluster by expiry
- Position netting and margin mostly benefits within the same underlying + expiry bucket
- Old expiries become “cold” quickly and can be settled + archived

---

## What goes into each shard

A shard owns:

- series metadata _for that bucket_ (or references Registry)
- positions for users trading those series
- margin accounts (or per-user subaccounts scoped to that shard)
- settlement for series in that bucket
- local pointers into TradeLog

**Example**
All of these land in the same shard:

- ETH call 2500 exp 30 Jun 2026
- ETH put 2000 exp 30 Jun 2026
- ETH binary “ETH > 3k?” exp 30 Jun 2026

All go to:
`(ETH_USD, 202606)`

---

## How digitals fit

Digitals are just series where:

- `payoff_type = Binary`
- `strike` optional / encoded (e.g. threshold)
- same expiry_month routing

So you’re not building a “separate product”.
You’re building series with one payoff model first.

---

## Shard sizing knobs

You can tune the bucket size if needed:

### Option A (default): **monthly**

`YYYYMM`
Best balance. Plenty of shards over time, but each shard is manageable.

### Option B: **weekly** (if you list lots of weeklies)

`YYYYWW`
Use if you expect huge weekly options volume and want smaller shards.

### Option C: **quarterly** (if low volume)

`YYYYQ`
Fewer shards, bigger shards. Good early-stage but might grow hot later.

You can start monthly and only add weekly for specific underlyings later.

---

## TradeLog + Index sharding (so history never hurts you)

### TradeLog shards

Shard TradeLog by **time** (e.g. monthly) or by **clearing shard_key**:

- `TradeLog_2026_06` (time-based)
  or
- `TradeLog_ETH_202606` (instrument-bucket-based)

Time-based is simpler for archiving and retention policies.

### Index shards

Shard by user hash prefix:

- `Index_00..0F`, `Index_10..1F`, etc.

Each stores pointers:

- `(user) -> list of (event_id ranges / TradeLog shard ids)`

---

## Portability (between clearings) with this sharding

Your novation flow becomes simple:

1. User requests transfer of `(series_id, qty)` from Clearing A → Clearing B
2. Clearing A freezes within the relevant shard `(underlying_id, expiry_month)`
3. Issues signed proof for that series/qty + margin state
4. Clearing B accepts and imports into its own shard for the same `(underlying_id, expiry_month)`

Because both clearings use the same shard_key derivation, they both know exactly where that position lives.

---

## Operational benefits

- **Upgrades are easier**: you can roll upgrades shard-by-shard.
- **Hot underlyings don’t DOS everything**: ETH June doesn’t impact BTC March.
- **Natural lifecycle**: once a bucket is fully settled, it becomes mostly read-only.
- **Growth is linear**: you add shards, not complexity.

---

## Minimal “Router” interface (what exchanges call)

Exchanges/OTC desks should never guess canister IDs. They call:

- `resolve_clearing(shard_key) -> clearing_id`
- then `submit_matched_trade(clearing_id, ...)`

Or even:

- `route_and_submit_trade(series_id, ...)` (router forwards)

If you want maximum neutrality, keep routing public and deterministic.

---

If you tell me the top 5 underlyings you’ll support initially (e.g. ICP/USD, BTC/USD, ETH/USD, CPI, rates), I’ll propose the exact initial shard plan (how many shards to pre-create, rollover rules, and what lives in router vs registry vs clearing).
