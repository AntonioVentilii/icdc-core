# Closed Circles: Restricted Trading Groups

This document describes the architecture for "closed circles" — a system that allows prediction markets to be restricted to specific groups of traders rather than being open to everyone.

## Overview

By default, every series (prediction market) is open to all authenticated users. With closed circles, an admin can restrict a series so that only members of designated groups may submit orders. Groups are managed on-chain in the registry canister and enforced by the clearing canister at order submission time.

A series can have **multiple trading access policies** simultaneously. If any policy grants access, the trader is authorized. This enables future extensibility (e.g., token-gated access, reputation-based access) without breaking existing policies.

## Architecture

```mermaid
flowchart TD
    subgraph app [vici-app]
        GroupCreatorUser["Group Creator / Admin"]
        GroupAdmin["Group Admin"]
        SiteAdmin["Site Admin"]
        Trader["Trader"]
    end

    subgraph registry [Registry Canister]
        GroupsStore["GROUPS_STORE"]
        SeriesStore["SERIES_STORE"]
        AuthCheck["is_trading_authorized"]
    end

    subgraph clearing [Clearing Canister]
        OrderSubmit["submit_limit/market_order"]
        AccessGate["check_trading_access"]
    end

    GroupCreatorUser -->|"create_group"| GroupsStore
    GroupAdmin -->|"update_group, add/remove admins/members"| GroupsStore
    SiteAdmin -->|"add_series with trading_access"| SeriesStore
    SiteAdmin -->|"update_trading_access"| SeriesStore
    Trader -->|"submit_limit_order"| OrderSubmit
    OrderSubmit --> AccessGate
    AccessGate -->|"Series is Open?"| SkipCheck["Allow immediately"]
    AccessGate -->|"Series is Restricted?"| AuthCheck
    AuthCheck -->|"Check group membership"| GroupsStore
    AuthCheck -->|"authorized"| AllowTrade["Accept Order"]
    AuthCheck -->|"not authorized"| RejectTrade["NotAuthorizedToTrade"]
```

## Data Model

### TradingAccess (shared types)

```rust
enum TradingAccess {
    Open,                                // anyone can trade
    Restricted { groups: Vec<GroupId> },  // only group members
}
```

A `Series` holds `trading_access: Vec<TradingAccess>`. **This list must never be empty** — every series carries at least one policy (default: `[Open]`). Authorization is evaluated as an OR across all policies: if any policy grants access, the caller can trade. The invariant is enforced by `add_series` (fills `[Open]` if empty) and `update_trading_access` (rejects empty with `EmptyTradingAccess`).

### Group

```rust
struct Group {
    group_id: GroupId,
    name: String,
    description: Option<String>,
    icon_url: Option<String>,
    creator: Principal,           // immutable, always treated as admin
    admins: BTreeSet<Principal>,  // can manage everything
    members: BTreeSet<Principal>, // can trade
    created_at_ns: u64,
    updated_at_ns: u64,
    updated_by: Principal,
}
```

Groups are stored in the registry's `GROUPS_STORE`. A group has three roles:

- **Creator** — recorded at creation, immutable, always treated as an admin.
- **Admins** — can manage other admins, manage members, update metadata, and delete the group. The creator is implicitly an admin even if not in the `admins` set.
- **Members** — can trade on series restricted to this group, nothing else.

The `admins` and `members` sets are independent: an admin is NOT automatically a member. If an admin should also trade, they must appear in both sets.

Every mutation stamps `updated_at_ns` and `updated_by` for audit purposes.

## Enforcement Flow

```mermaid
sequenceDiagram
    participant Trader
    participant Clearing
    participant Registry

    Trader->>Clearing: submit_limit_order(series_id, ...)
    Clearing->>Clearing: ensure_series_registered(series_id)

    alt Series has Open policy
        Clearing->>Clearing: Skip access check (zero overhead)
    else Series has only Restricted policies
        Clearing->>Registry: is_trading_authorized(caller, series_id)
        Registry->>Registry: Check group membership
        alt Authorized
            Registry-->>Clearing: true
        else Not authorized
            Registry-->>Clearing: false
            Clearing-->>Trader: Err(NotAuthorizedToTrade)
        end
    end

    Clearing->>Clearing: validate_no_arbitrage, margin checks
    Clearing-->>Trader: Order accepted
```

Key design decisions:

1. **Zero overhead for open markets.** The clearing canister caches the series and checks `trading_access` locally. If the series contains an `Open` policy, no inter-canister call is made.

2. **No stale membership cache.** Group membership can change at any time. The clearing canister always queries the registry for restricted series rather than caching group members. Only the `TradingAccess` policy type is cached on the series.

3. **Controllers bypass.** Registry controllers are always authorized, regardless of group membership.

## Registry API (Groups)

| Endpoint                                           | Guard             | Description                                              |
| -------------------------------------------------- | ----------------- | -------------------------------------------------------- |
| `create_group(CreateGroupParams)`                  | Any authenticated | Creates a group; caller becomes creator and first member |
| `update_group(UpdateGroupParams)`                  | Group admin       | Updates group metadata (name, description, icon)         |
| `add_group_admins(UpdateGroupAdminsParams)`        | Group admin       | Adds principals to the admin set                         |
| `remove_group_admins(UpdateGroupAdminsParams)`     | Group admin       | Removes principals from the admin set                    |
| `add_group_members(UpdateGroupMembersParams)`      | Group admin       | Adds principals to the member set                        |
| `remove_group_members(UpdateGroupMembersParams)`   | Group admin       | Removes principals from the member set                   |
| `delete_group(GroupId)`                            | Group admin       | Deletes a group                                          |
| `get_group(GroupId)`                               | Public query      | Returns group details                                    |
| `list_groups(Option<Principal>)`                   | Public query      | Lists groups, optionally filtered by creator             |
| `is_group_member(GroupId, Principal)`              | Public query      | Checks membership                                        |
| `is_trading_authorized(Principal, SeriesId)`       | Public query      | Resolves all policies for a series                       |
| `update_trading_access(UpdateTradingAccessParams)` | Controller only   | Changes trading access on a series                       |

**"Group admin"** means: canister controller, group creator, or a principal in the group's `admins` set.

## vici-app Roles

| Role                   | Permissions                                                          |
| ---------------------- | -------------------------------------------------------------------- |
| `CONTROLLER` / `ADMIN` | All permissions including `CREATE_GROUP` and `MANAGE_TRADING_ACCESS` |
| `GROUP_CREATOR`        | `CREATE_GROUP` — can create and manage groups                        |
| `CREATOR`              | `CREATE_MARKET` — cannot manage trading access (admin does that)     |

## Future Extensions

The `TradingAccess` enum is designed for extensibility. Potential future variants:

- `TokenGated { token: Principal, min_balance: u128 }` — require holding a minimum token balance
- `ReputationBased { min_score: u64 }` — require a minimum reputation score
- `TimeLimited { open_from_ns: u64, open_until_ns: u64 }` — time-windowed access

Each new variant only requires adding the enum case in shared types, the authorization logic in `is_trading_authorized`, and UI support in the app.
