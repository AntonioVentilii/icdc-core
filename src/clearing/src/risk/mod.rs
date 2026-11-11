//! Risk engine, phase 1 of `docs/risk/margin-model.md`: roles, bounds, timelock, price history and
//! volatility.
//!
//! Nothing in this module feeds margin yet; see `docs/plans/margin-phase-1-implementation.md`.

pub mod calendar;
pub mod fixed;
pub mod history;
pub mod params;
pub mod sampler;
pub mod state;
pub mod timelock;
pub mod volatility;
