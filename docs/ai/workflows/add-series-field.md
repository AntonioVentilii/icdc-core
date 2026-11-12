# Workflow: add a field to `Series` (or any saved type)

A recurring change: `optional start_ns for scheduled series` (#92), `optional locale tag to Series` (#44), `compulsory Series.resolution with backfill migration` (#68). `Series` is saved by the registry and cached by the clearing, so one field touches both canisters.

1. **Decide `opt` or compulsory.** An `Option<T>` field needs no migration: Candid decodes old state with `None`. A compulsory field, a rename or a type change needs a migration (step 5). Prefer `opt` unless the owner wants the field mandatory.
2. **Add the field** to `Series` in `src/shared/src/types/series.rs`, with a doc comment that says what `None` means. A length limit or default goes in `src/shared/src/constants.rs` (as `MAX_LOCALE_LEN`, `DEFAULT_LOCALE`), never inline.
3. **Decide whether it is part of the identity.** `Series::generate_id` hashes the fields that make a series unique (`start_ns` and `expiry_ns` are, which is why the clearing can cache them forever). Adding a field to `SeriesIdParams` changes every new id: ask the owner first.
4. **Accept it on creation and, if editable, on update**: `AddSeriesParams` (and `ForkSeriesParams` if forks inherit it), validated in `add_series_impl` in `src/registry/src/api/series.rs`; an editable field also goes into `UpdateSeriesMetadataParams` and `update_series_metadata_impl`. Unit-test the validation there.
5. **Migrate if compulsory**, following [`../migrations.md`](../migrations.md): the field's backfill rule in `src/shared/src/migrations/mod.rs` (`LegacySeries`, `upgrade_series`), wired into both `src/registry/src/migrations/` and `src/clearing/src/migrations/`. Even an `opt` field needs `upgrade_series` to set it, since it builds a `Series` literal.
6. **Use it in the clearing** only where the behaviour needs it (for example the start gate in `src/clearing/src/utils/series.rs`).
7. **Update every `Series { ... }` literal.** There is no shared test builder yet, so the compiler will point at about fifteen: the registry and clearing tests, `src/clearing/src/api/trade/tests/`, `src/clearing/tests/it/utils/trade_helper.rs` and the migrations. Fill the new field explicitly; do not add `..Default::default()`.
8. **Regenerate Candid** with `npm run did` last; both `registry.did` and `clearing.did` change.
9. **PR**: `feat(shared,registry): ...` (add `clearing` to the scope when the clearing's behaviour changes).
