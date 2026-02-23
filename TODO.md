# Clearing

1. Add a cron job to register all the supported series from method list_series of registry.
2. Add guards so that the methods are called correctly by the correct users.
3. Support multi-asset collateral/settlement.
4. Support other networks/assets.
5. Check security overall.
6. Cron-job to "refresh" the balance of the users on each asset.
7. Better pattern for settlement:
   1. Compute & store a settlement plan in stable state (idempotency key / settlement id, per-user amounts, status).
   2. Execute transfers with idempotent logic:
      - record per-user “paid” flags or store block indexes returned by ledger
      - on retry, skip already-paid
   3. Only once fully complete, finalise:
      - clear positions
      - update margin accounts
8. Who should pay the fees during settlement: Receiver economically pays payout fee
9. Make fees explicit and consistent

### Pre-check solvency before doing transfers

Before executing:

- total required from payers’ subaccounts (including their fees) should be available
- pool should have enough to pay receivers + total payout fees
  If not, fail early before doing any transfers.

# Registry

1. Should the methods to add be called only by a controller? Or open to everybody? Shall i limit the number of request/s per principal?

# Idempotency - The clean version of the pattern

### Phase A — Build plan (deterministic, no awaits)

Create a `SettlementPlan` record in stable state:

- `settlement_id` (idempotency key)
- market/series id
- list of `(user, amount, asset, to_account)`
- `status: Planned | Executing | Finalised`
- per-user payment state:
  - `payment_block_index: Option<Nat>` or `paid: bool`

- optionally: `created_at_time` per transfer for idempotency

**Important:** the plan should be derivable deterministically from (market id, resolution timestamp, snapshot of positions). Store the snapshot hash or the snapshot itself.

### Phase B — Execute transfers (async, resumable)

Iterate over plan entries:

- if `paid` / `payment_block_index.is_some()` → skip
- call ledger transfer
- if success or Duplicate → record block index
- persist after each payment

So if you trap after paying user #37, you resume at #38.

### Phase C — Finalise (no awaits)

Only when _all_ entries are marked paid:

- clear/settle positions
- update internal collateral balances
- mark plan as `Finalised`

That ensures “money moved” before “positions cleared”.

---

# Key design choices to make it actually bulletproof

## 1) You must persist progress _after each_ successful payment

Not just at the end of a batch.

Otherwise one trap loses the progress and you’ll double-pay on retry.

## 2) You need idempotency at the ledger layer too

Use `created_at_time` and treat `Duplicate{duplicate_of}` as success, exactly like deposits.

This matters if:

- you sent a transfer
- you trapped before saving the returned block index
- on retry you would send again unless the ledger dedupes

So for each payout entry store a `created_at_time` once and reuse it on retries.

## 3) Make finalisation idempotent

Finalisation should be safe to run twice:

- check `status != Finalised`
- ensure `all_paid == true`
- then apply final state transition

## 4) Don’t mutate “margin accounts” during execution

During execution you are in an in-between world. Keep the canonical risk state frozen until finalisation.

---

# One subtle but important point

Your suggested last step says:

> “Only once fully complete, finalise: clear positions, update margin accounts”

✅ Yes, but the ordering inside finalise matters depending on your model.

If settlement _pays out from the canister’s custody_:

- update internal balances first (credit winners / debit losers)
- then clear positions
- (both in the same finalise step, no awaits)

But if your settlement _already moved funds externally_ (i.e., direct ledger transfers to users), then your internal balances must reflect that:

- reduce canister-held balances accordingly
- clear positions

The key is: the finalise step must bring internal accounting back into alignment with what actually happened on-ledger.

---

# In short

Your 3-step pattern is the correct settlement architecture for IC:

- **Plan (durable)**
- **Execute (idempotent + resumable)**
- **Finalise (single atomic state transition)**

It’s the same pattern you should use for:

- batch withdrawals
- liquidations that involve multiple transfers
- multi-asset settlements
