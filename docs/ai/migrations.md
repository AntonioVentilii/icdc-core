# Stable-State Migrations

This document defines the convention for **stable-state schema migrations** in
`icdc-core`: where migration code lives, how it is structured, and when it is
retired. Follow it whenever you change the shape of any type that is persisted
across canister upgrades.

## Why migrations need care

Every canister persists its heap state to stable memory with **candid**
(`ic_cdk::storage::stable_save` in `pre_upgrade`, `stable_restore` in
`post_upgrade`). The trap to remember:

> **Candid record decoding fails on a missing _non-optional_ field.**

So if you add a compulsory (non-`opt`) field to a persisted type, the next
upgrade decodes the _old_ bytes (which lack the field) against the _new_ type
and **traps in `post_upgrade` — bricking the canister / losing state**. Adding
an `opt` field is safe (candid defaults it to `null`); adding a non-`opt` field,
renaming, or changing a field type is **not** and requires a migration.

## Two distinct things both called "migration"

| Concept                                | What it is                                                                                | Where it lives                                                                  |
| -------------------------------------- | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| **Stable-state migration** (this doc)  | A one-shot transform of persisted state across an upgrade (e.g. backfilling a new field). | `src/<crate>/src/migrations/` + the shared rule in `src/shared/src/migrations/` |
| **Domain migration** (runtime feature) | A live, user-facing API operation (e.g. moving a user between balance domains).           | `src/clearing/src/api/migration/` (a normal `api/` domain)                      |

Do not conflate them. Stable-state migrations are transient upgrade plumbing;
domain migrations are permanent product features.

## The convention

1. **Dedicated, quarantined module.** All stable-state migration code lives in a
   `migrations/` module, never mixed into live `api/` or domain modules. This
   keeps transient code easy to find and delete.
   - Canister-agnostic rules (pure transforms, legacy element shadow types reused
     by more than one canister) go in **`src/shared/src/migrations/`**.
   - Per-canister wiring (legacy `StableState*` shadow structs, the
     `restore_state` fallback) goes in **`src/<crate>/src/migrations/`** and is
     called from that canister's `post_upgrade` / `memory::restore_state`.

2. **Decode-old → transform → store-new.** Mirror the historical type with a
   `Legacy*` shadow (`#[derive(CandidType, Deserialize)]`) that matches the old
   shape exactly. On restore, try the current schema first; on failure, decode
   the legacy shadow and map it onto the current type, backfilling new fields.
   The registry's versioned fallback chain (`StableStateV5` → `LegacyStableStateV4`
   → V3 → V2 → tuple) and the clearing single-step fallback (`StableState` →
   `LegacyStableState`) are the reference examples.

3. **Header doc-comment with a retirement note.** Every migration module states
   _what_ it backfills, _which version range_ it bridges, and _when it can be
   deleted_ (once every deployed canister has upgraded past the introducing
   release and no pre-migration blob can exist). When that condition holds, the
   module and its `Legacy*` types should be removed.

4. **Test the round-trip.** Encode a `Legacy*` value, assert it does **not**
   decode against the new schema (proving the migration is necessary and the
   shadow shape is right), then decode it as legacy, run the transform, and
   assert the backfill and that all other state survives. See the `#[cfg(test)]`
   modules in the `migrations/` files.

## Reference implementation: `Series::resolution`

The compulsory `Series::resolution` field was added with:

- `shared::migrations` — `LegacySeries` (pre-`resolution` shape),
  `resolution_from_description` (copies `description.plain`, else the
  `"no resolution data"` placeholder), and `upgrade_series`.
- `registry::migrations` — `LegacyStableStateV{4,3,2}` + `upgrade_series_map`,
  wired into `registry::memory::restore_state` after the `StableStateV5` attempt.
- `clearing::migrations` — `LegacyStableState` + `into_current`, wired into
  `clearing::memory::restore_state` as the fallback decode.

These are one-shot and slated for deletion once all canisters are past the
introducing release.

## Checklist for a schema change

- [ ] Is the new field `opt`? If yes, no migration needed — just add it.
- [ ] If non-`opt` / renamed / retyped: add a `Legacy*` shadow and a transform.
- [ ] Put the rule in `shared/migrations` if more than one canister needs it.
- [ ] Wire the fallback into each affected canister's `restore_state`.
- [ ] Add a retirement note to the module header.
- [ ] Add round-trip tests.
- [ ] Run `npm run did` (candid changes) and `npm run quality`.
