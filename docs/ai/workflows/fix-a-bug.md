# Workflow: fix a bug

Fixes are a large share of the history (`fix(shared): correct off-by-one in PaginationParams cursor`, `fix(clearing): send explicit fee on ICRC transfers with cached-fee + BadFee retry`, `fix(clearing): forbid administrative withdrawals from the insurance fund`). Every one follows the same four steps.

1. **Reproduce with a failing test.** Write the test that shows the bug before touching the code, at the lowest layer that can show it: a unit test on the pure core when the logic is wrong, an integration test in `src/clearing/tests/it/` when the bug needs a ledger, an upgrade or two canisters. Name it after the correct behaviour ([`../testing.md`](../testing.md#naming)).
2. **Find the root cause.** Trace why, not only where. For anything touching value, check the invariants in [`../domain.md`](../domain.md#8-invariants): did a failed path leave partial state, did an await sit between validation and mutation, did an amount come from internal accounting where it should come from the ledger?
3. **Fix the smallest correct thing.** Change the owner of the logic, not each caller. If the same mistake exists in a copy (see the anti-patterns in [`../patterns.md`](../patterns.md#anti-patterns-found-in-the-code)), fix every copy in this PR or make them one (rule 5 of the standard). A refactor the fix does not need is a separate PR.
4. **Verify.** The new test passes, the whole suite passes (`npm run test:unit`, `npm run test:integration`), `npm run quality` is clean. If the fix changes an endpoint's behaviour or error shape, run `npm run did` and say so in the PR body.

Title: `fix(<scope>): <what is now correct>`. A fix in a sensitive module ([`../governance.md`](../governance.md#sensitive-code-the-deepest-review)) explains the failure scenario under `# Motivation`.
