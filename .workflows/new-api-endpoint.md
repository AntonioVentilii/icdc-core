---
description: Create a new API endpoint in the Clearing canister
---

Follow these steps to add a new API endpoint (e.g., `GetAccountHistory`):

1. **Define Params, Results, and Errors**:
   - Create/Update `src/clearing/src/api/[domain]/params.rs` with the `[Action]Params` struct.
   - Create/Update `src/clearing/src/api/[domain]/results.rs` with the `[Action]Result` or `[Action]Response` struct.
   - Create/Update `src/clearing/src/api/[domain]/errors.rs` with the specific `[Domain]Error` enum if needed.
   - Ensure all structs and enums derive `CandidType, Serialize, Deserialize, Clone, Debug`.

2. **Implement Domain Logic**:
   - In `src/clearing/src/[domain]/service.rs`, implement the business logic inside the `[Domain]Service` struct.
   - Follow the **Validation -> Mutation** atomic pattern.
   - Write unit tests in the `tests` module at the bottom of `service.rs`.

3. **Bridge into API Layer**:
   - In `src/clearing/src/api/[domain]/mod.rs`, add the `update` or `query` function.
   - Extract global state (e.g., `ACCOUNT_STATES.with(|...| ...)`) and pass it to the domain service.
   - Return the result from the service.

4. **Expose in Canister Entry Point**:
   - In `src/clearing/src/lib.rs`, import the new params/results.
   - Add the `#[update]` or `#[query]` function that calls the API module.

5. **Update Candid File**:
   - Run `cargo test` (if using `export_candid!`) or manually update the `.did` file.

6. **Verify**:
   - Run the unit tests in `service.rs`.
   - Add a manual verification step in your `walkthrough.md`.
