# IC Derivatives Clearing (ICDC)

ICDC is a decentralised, multi-canister clearing infrastructure built on the Internet Computer. It provides a neutral, scalable, and non-monopolistic environment for derivative instruments.

## 🏗 Project Structure

The project is divided into two main functional areas:

- **[Series Registry](src/registry/README.md)**: A global directory for instrument specifications and oracle sources.
- **[Clearing Engine](src/clearing/README.md)**: The risk and margin engine that acts as a Central Counterparty (CCP) for trades.
- **[Shared Library](src/shared/)**: Common types and constants used across all canisters.

## 🚀 Key Features

- **Standardised Clearing**: Standardised interfaces for trade submission and position netting.
- **Position Portability**: A unique novation protocol that allows positions to move between different clearing canisters without vendor lock-in.
- **Async Safety**: Implements a robust 3-step idempotency pattern for all ledger interactions.
- **Multi-Asset Support**: Built-in support for multiple ledger-based collateral assets.

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
