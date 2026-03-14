# Project Flow Schematics

This document provides high-level schematics of the core functional flows within the ICDC ecosystem. These diagrams use Mermaid syntax to illustrate the interactions between users, the ICDC canisters (Registry, Clearing, Minter), and external LEDGERS.

## 1. Initialisation Flow

The initialisation process sets up the environment before any trading can occur.

```mermaid
sequenceDiagram
    participant Admin
    participant Registry
    participant Clearing
    participant Ledger as vUSD Ledger

    Note over Admin, Ledger: 1. Setup Registry
    Admin->>Registry: add_authorized_creators([Admin])
    Admin->>Registry: add_oracle(TRADE_ORACLE, [Admin])

    Note over Admin, Ledger: 2. Setup Clearing
    Admin->>Clearing: set_registry_canister(Registry)
    Admin->>Clearing: register_icrc_asset(TESTICP, Ledger)
    Admin->>Clearing: update_asset_price(TESTICP, $3.00)

    Note over Admin, Ledger: 3. Setup Series
    Admin->>Registry: add_series(BTC/USD, Expiry, Call, Strike)
    Registry-->>Admin: Returns SeriesId
```

## 2. Trading Flow

### 2.1 Submit Limit Order (Maker)

An atomic operation that reserves margin before placing the order in the book.

```mermaid
sequenceDiagram
    participant Maker
    participant Clearing

    Maker->>Clearing: submit_limit_order(SeriesId, Outcome, Price, Qty)
    Clearing->>Clearing: calculate_required_margin()
    Clearing->>Clearing: check_solvency(Maker)
    alt is_solvent
        Clearing->>Clearing: reserve_margin(Maker)
        Clearing->>Clearing: insert_limit_order(Book)
        Clearing-->>Maker: Ok(OrderId)
    else insufficient_margin
        Clearing-->>Maker: Err(InsufficientMargin)
    end
```

### 2.2 Submit Market Order (Taker)

Matches an existing limit order and executes the trade atomically.

```mermaid
sequenceDiagram
    participant Taker
    participant Clearing

    Taker->>Clearing: submit_market_order(OrderId, TradeId)
    Clearing->>Clearing: get_order(OrderId) from Internal Book
    Clearing->>Clearing: check_solvency(Taker)
    alt is_solvent
        Clearing->>Clearing: execute_trade_impl()
        Note right of Clearing: Update Positions (Maker & Taker)
        Note right of Clearing: Update Cash Balances (PnL)
        Note right of Clearing: Release/Update Reserved Margin
        Clearing->>Clearing: remove_order(OrderId) from Internal Book
        Clearing-->>Taker: Ok(True)
    else insufficient_margin
        Clearing-->>Taker: Err(InsufficientMargin)
    end
```

## 3. Collateral Management (Plan-Execute-Finalise)

Collateral operations are multi-phase to ensure safety and idempotency in an asynchronous environment.

### 3.1 Deposit Collateral

```mermaid
sequenceDiagram
    participant User
    participant Clearing
    participant Ledger as vUSD Ledger

    Note over User, Ledger: Phase A: Build Plan
    User->>Clearing: deposit_collateral(Asset, Amount, DepositId)
    Clearing->>Clearing: create_deposit_plan(Status: Executing)

    Note over User, Ledger: Phase B: Execute Transfer
    Clearing->>Ledger: icrc2_transfer_from(User -> Clearing)
    Ledger-->>Clearing: Ok(BlockIndex)
    Clearing->>Clearing: update_plan(Receipt: BlockIndex)

    Note over User, Ledger: Phase C: Finalise
    Clearing->>Clearing: credit_user_balance(InternalState)
    Clearing->>Clearing: mark_plan(Status: Finalised)
    Clearing-->>User: Ok()
```

### 3.2 Withdrawal Collateral

```mermaid
sequenceDiagram
    participant User
    participant Clearing
    participant Ledger as vUSD Ledger

    Note over User, Ledger: Phase A: Build Plan & Risk Check
    User->>Clearing: withdraw_collateral(Asset, Amount, WithdrawalId)
    Clearing->>Clearing: check_post_withdrawal_solvency()
    Clearing->>Clearing: debit_internal_balance()
    Clearing->>Clearing: create_withdrawal_plan(Status: Executing)

    Note over User, Ledger: Phase B: Execute Transfer
    Clearing->>Ledger: icrc1_transfer(Clearing -> User)
    Ledger-->>Clearing: Ok(BlockIndex)
    Clearing->>Clearing: update_plan(Receipt: BlockIndex)

    Note over User, Ledger: Phase C: Finalise
    Clearing->>Clearing: mark_plan(Status: Finalised)
    Clearing-->>User: Ok()

    Note over Clearing, Ledger: On Failure: Refund internal balance
```

## 4. Inter-canister Interaction Map

```mermaid
graph TD
    User((User))
    Registry[Registry Canister]
    Clearing[Clearing Canister]
    Minter[Minter Canister]
    Ledger[[vUSD Ledger]]
    Oracle((Oracle Source))

    User -- "1. Discover Series" --> Registry
    User -- "2. Deposit / Trade" --> Clearing
    Clearing -- "3. Verify Series" --> Registry
    Clearing -- "4. Settle / Mint" --> Minter
    Clearing -- "5. Transfer" --> Ledger
    Minter -- "6. Transfer (Payout)" --> Ledger
    Registry -- "7. Fetch Data" --> Oracle
```

> [!NOTE]
> **Internal vs. External Settlement**
>
> In ICDC, "Trading" and "Clearing" are decoupled from "Settlement" on the main ledger.
>
> - **Internal (Trading/Netting)**: When a trade is executed, only the internal `AccountState` and `Positions` within the Clearing canister are updated. This allows for sub-second, atomic execution without waiting for ledger blocks.
> - **External (Deposit/Withdrawal)**: The `vUSD Ledger` is only involved when a user moves funds in or out of the clearing infrastructure.

## 5. Minter & vUSD Ledger Details

### 5.1 vUSD Ledger Initialisation

The `vUSD Ledger` is a standard ICRC-1 ledger.

```mermaid
sequenceDiagram
    participant Admin
    participant Ledger as vUSD Ledger
    participant Archive as Ledger Archive

    Admin->>Ledger: deploy(Wasm, InitArgs)
    Note right of Ledger: Args: Name, Symbol, Decimals,<br/>Minting Account, Initial Balances
    Ledger->>Archive: spawn_archive()
```

### 5.2 Minter Initialisation

The Minter acts as a permissioned bridge to the Ledger.

```mermaid
sequenceDiagram
    participant Admin
    participant Minter
    participant Ledger as vUSD Ledger

    Admin->>Minter: deploy(Config)
    Note right of Minter: Config: authorized_principals,<br/>ledger_canister_id
    Admin->>Minter: update_config(NewConfig)
```

### 5.3 Minting / Payout Flow

Highly restricted process to move funds from the system's "Minting" source to users.

```mermaid
sequenceDiagram
    participant Authority
    participant Minter
    participant Ledger as vUSD Ledger
    participant User

    Authority->>Minter: mint(to: User, amount: 100)
    Note over Minter: check_caller_authorized()
    alt authorized
        Minter->>Ledger: icrc1_transfer(from: Minter, to: User, amount: 100)
        Ledger-->>Minter: Ok(BlockIndex)
        Minter-->>Authority: Ok(BlockIndex)
    else unauthorized
        Minter-->>Authority: Err(Unauthorized)
    end
```

> [!NOTE]
> **Minting Permissions**
>
> In the production environment, the `Minter` canister's principal would typically be the `minting_account` of the `vUSD Ledger` OR hold a significant balance designated for payouts. Access to the `mint` method is strictly gated.

## 6. Consumer Application Flow

A typical consumer application (e.g., a Trading Front-end or Liquidity Provider) acts as the bridge between the end-user and the ICDC infrastructure.

```mermaid
sequenceDiagram
    participant User
    participant App as Consumer App (Frontend)
    participant Registry as ICDC Registry
    participant Clearing as ICDC Clearing

    Note over User, Clearing: 1. Discovery Phase
    App->>Registry: list_series(Pagination)
    Registry-->>App: Series[]
    App->>User: Display available markets

    Note over User, Clearing: 2. User Onboarding
    User->>App: Connect Identity (II / Plug / etc)
    App->>Clearing: get_account_state(User)
    Clearing-->>App: Balance, Margin, Positions
    App->>User: Show Wallet & Portfolio

    Note over User, Clearing: 3. Trading & Liquidity
    App->>Clearing: list_orders(SeriesId)
    Clearing-->>App: LimitOrders[]
    App->>User: Display Order Book

    User->>App: "Place Buy Order"
    App->>Clearing: submit_limit_order(Params)
    Clearing-->>App: Ok(OrderId)

    Note over User, Clearing: 4. Execution Monitoring
    loop Periodic Update
        App->>Clearing: get_orders()
        Clearing-->>App: ActiveOrders[]
        App->>Clearing: get_trade_history()
        Clearing-->>App: ExecutedTrades[]
    end
    App->>User: Notify "Order Filled / Partially Filled"
```
