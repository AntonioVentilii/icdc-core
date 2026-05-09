#!/bin/bash
set -euo pipefail

# `cargo nextest` runs each test in its own process and parallelises
# more aggressively than `cargo test`, which cuts ~30-50% off the
# wallclock for our PocketIC-heavy integration suite. It also gives
# better failure summaries when a panic happens deep in a `TestSetup`
# helper. We fall back to `cargo test` when nextest is not on PATH so
# this script keeps working for contributors who haven't run
# `scripts/setup cargo-nextest` yet.
if cargo nextest --version >/dev/null 2>&1; then
  RUST_BACKTRACE=1 RUSTFLAGS="-D warnings" cargo nextest run --test it "${@}"
else
  echo "cargo-nextest not found, falling back to cargo test" >&2
  echo "Run 'scripts/setup cargo-nextest' to opt in." >&2
  RUST_BACKTRACE=1 RUSTFLAGS="-D warnings" cargo test --test it "${@}"
fi
