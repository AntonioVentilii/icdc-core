#!/usr/bin/env bash

set -euxo pipefail

cd "$(dirname "$(realpath "$0")")/.."

set -x
time (
  ./scripts/lint.cargo-workspace-dependencies.sh
  ./scripts/lint.did.sh
  ./scripts/lint.github.sh
  ./scripts/lint.rust.sh
  ./scripts/lint.sh.sh
)
