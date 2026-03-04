# Series Registry Canister

The Series Registry is a neutral, public directory for derivative contract series. It provides a centralised location for discovering instruments and their canonical parameters.

## Core Responsibilities

- **Registration**: Allows for the creation of new derivative series with deterministic IDs.
- **Discovery**: Provides query methods to search and list series with advanced cursor-based pagination.
- **Metadata Management**: Stores underlying asset, expiry, payoff type, strike, settlement asset, and oracle information.

## API Overview

### Updates

- `add_series(params)`: Registers a new series. Returns the canonical `SeriesId`.
- `add_oracle(params)`: Registers a new oracle source.
- `update_oracle_metadata(params)`: Updates metadata for an existing oracle.

### Queries

- `get_series(series_id)`: Retrieves a specific series by its ID.
- `list_series(pagination)`: Returns a paginated list of all series.
- `list_series_with(params)`: Returns a paginated list of series filtered by custom criteria.
- `get_oracle(oracle_id)`: Retrieves oracle metadata.

## Current Limitations & Roadmap

- **[MISSING] Access Control**: Currently open to all non-anonymous callers. Future updates will restrict `add_series` to authorized principals.
- **[MISSING] Rate Limiting**: Protection against spam registration is not yet implemented.
- **[PLANNED] Governance Integration**: Transition ownership and authorized creator management to a DAO/governance model.
