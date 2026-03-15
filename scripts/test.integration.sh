#!/bin/bash
set -euo pipefail

RUST_BACKTRACE=1 RUSTFLAGS="-D warnings" cargo test --test it
