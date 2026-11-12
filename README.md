# IC Derivatives Clearing (ICDC)

ICDC is a decentralised, multi-canister clearing infrastructure built on the Internet Computer. It provides a neutral, scalable, and non-monopolistic environment for derivative instruments.

## 🏗 Project Structure

The project is divided into two main functional areas:

- **[Series Registry](src/registry/README.md)**: A global directory for instrument specifications and oracle sources.
- **[Clearing Engine](src/clearing/README.md)**: The risk and margin engine that acts as a Central Counterparty (CCP) for trades.
- **[Shared Library](src/shared/)**: Common types and constants used across all canisters.
- **[Architecture Flows](docs/architecture/flows.md)**: Visual schematics of the core system processes.
- **[Balance domains](docs/architecture/balance-domains.md)**: Core vs app-specific balance domains (settlement, playground, optional branded domains).

## 🚀 Key Features

- **Standardised Clearing**: Standardised interfaces for trade submission and position netting.
- **Position Portability**: A unique novation protocol that allows positions to move between different clearing canisters without vendor lock-in.
- **Async Safety**: Implements a robust 3-step idempotency pattern (`Plan-Execute-Finalise`) for all ledger interactions.
- **Multi-Asset Support**: Built-in support for multiple ledger-based collateral assets.

## ⚖️ Design Philosophy: Why a CCP?

Unlike many DeFi protocols that tokenise positions (e.g., ERC20 options), ICDC follows a **Central Counterparty (CCP)** model typical of mature fiat derivatives infrastructure.

- **Risk Centralisation**: By keeping positions internal to the clearing canister rather than tokenising them, we enable complex portfolio netting, efficient margin calculations, and sophisticated risk management that "detached" tokens cannot support.
- **Portability via Novation**: Instead of free-market token transfers, we support position portability through a controlled **Novation Protocol**, preserving risk integrity across the ecosystem.
- **Neutrality**: The clearing engine remains independent of UIs, exchanges, and specific token logic, serving as a pure risk engine for any platform.

## 🛠 Getting Started

### Prerequisites

- [DFX](https://internetcomputer.org/docs/current/developer-docs/setup/install) (latest version)
- [Rust](https://www.rust-lang.org/tools/install) (stable)

### Development

```bash
# Start the local replica
dfx start --background --clean

# Deploy all canisters
dfx deploy
```

## 🗺 Roadmap & Status

See the **[TODO.md](TODO.md)** for a detailed list of completed features, missing items, and future suggestions.
