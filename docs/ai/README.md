# docs/ai

The agent docs of `icdc-core`. Start from [`AGENTS.md`](../../AGENTS.md); the universal rules are in the [standard](https://github.com/AntonioVentilii/officina/blob/main/standards/code.md). These pages add only what is specific to this repository.

## Working rules

| Page                               | What it holds                                                           |
| ---------------------------------- | ----------------------------------------------------------------------- |
| [`governance.md`](governance.md)   | Generated and protected paths, what needs approval, the meta-update map |
| [`structure.md`](structure.md)     | Crates, folders, where new code goes, dependency direction              |
| [`patterns.md`](patterns.md)       | The patterns to copy, with examples, and the anti-patterns not to       |
| [`reusability.md`](reusability.md) | The catalog of existing types, helpers, constants, guards and fixtures  |
| [`domain.md`](domain.md)           | The economic rules: atomicity, settlement, vault, vUSD, invariants      |
| [`migrations.md`](migrations.md)   | How persisted state changes across upgrades                             |
| [`testing.md`](testing.md)         | Unit and integration tests: where, how, commands                        |
| [`pr-and-ci.md`](pr-and-ci.md)     | PR title and body, local gates, CI jobs, updating and reviewing PRs     |

## Workflows

- [`workflows/new-endpoint.md`](workflows/new-endpoint.md): add a clearing or registry endpoint.
- [`workflows/add-series-field.md`](workflows/add-series-field.md): add or change a field on `Series` or another saved type.
- [`workflows/fix-a-bug.md`](workflows/fix-a-bug.md): reproduce, fix, verify.

## Feature notes

Design notes written with the features they describe; read the one for the area you touch.

- [`open-markets-query.md`](open-markets-query.md): server-side open-markets discovery.
- [`series-trade-history-query.md`](series-trade-history-query.md), [`series-price-history-query.md`](series-price-history-query.md): series history queries.
- [`leaderboard-standings-query.md`](leaderboard-standings-query.md): per-window leaderboard.
- [`internal_ledger_verification.md`](internal_ledger_verification.md): verified vUSD properties.

Elsewhere: system design in [`NEW_ARCHITECTURE.md`](../../NEW_ARCHITECTURE.md), [`SHARDING_PLAN.md`](../../SHARDING_PLAN.md), [`docs/architecture/`](../architecture/); decisions in [`docs/adr/`](../adr/); release and deploy in [`docs/ci-cd/release-deploy.md`](../ci-cd/release-deploy.md); roadmap in [`TODO.md`](../../TODO.md).
