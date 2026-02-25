#!/usr/bin/env bash

RUST_BACKTRACE=1 RUSTFLAGS="-D warnings" cargo test --all-features
