#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$(realpath "$0")")/.."

set -x
# cargo-sort rewrites every Cargo.toml; `cargo fmt` reads them via `cargo metadata`, so it must run first.
time ./scripts/format.cargo.sh
time xargs -P8 -I{} bash -c "{}" <<EOF
./scripts/format.rust.sh
./scripts/format.sh.sh
EOF
