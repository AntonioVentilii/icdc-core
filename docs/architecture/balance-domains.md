# Balance domains

The clearing canister partitions collateral, margin, and per-series trading into **`BalanceDomain`** values (`shared::types::domain`). Domains are protocol-level labels; the engine does not know about specific apps or brands.

## Core domains (default everywhere)

| Domain           | Role                                                         |
| ---------------- | ------------------------------------------------------------ |
| **`Settlement`** | Production collateral (e.g. mainnet ICP, ckUSDC).            |
| **`Playground`** | Testnet / sandbox collateral (e.g. TESTICP, Sepolia assets). |

**Defaults** — including `AllowedBalanceDomains::default()` and the placeholder `internal_ledger` entry in clearing config — include **only** these two. New collateral assets inherit that unless an admin sets `allowed_balance_domains` explicitly.

## App-specific / branded domains

Additional variants (e.g. **`ViciXp`** for VICI loyalty points) exist so **points-only** margin can stay separate from **`Playground`** test assets, without a second clearing canister.

Properties:

- They are **opt-in**: not part of default allowlists. Operators attach them when registering or updating a collateral asset (`register_icrc_asset`, `update_collateral_allowed_domains`).
- **Series** in the registry declare a `balance_domain`; markets for XP use the branded domain, testnet markets use `Playground`, production uses `Settlement`.
- Adding a **new** named domain is a **code change**: extend `BalanceDomain` in `src/shared`, bump Candid (`.did` files), deploy; then wire assets and series to that domain.

Consumer apps stay out of the protocol: the clearing only sees domains and asset allowlists.

## Related code

- `src/shared/src/types/domain.rs` — `BalanceDomain`, `AllowedBalanceDomains`, `DomainPolicy`
- `src/clearing/src/api/admin/api.rs` — `register_icrc_asset`, `update_collateral_allowed_domains`, `update_domain_policy`
