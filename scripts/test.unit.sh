#!/usr/bin/env bash
set -euo pipefail

RUST_BACKTRACE=1 RUSTFLAGS="-D warnings" cargo test --lib
RUST_BACKTRACE=1 RUSTFLAGS="-D warnings" cargo test --bins
