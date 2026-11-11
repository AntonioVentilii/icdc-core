#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$(realpath "$0")")/.." || exit

print_help() {
  cat <<-EOF

	Fails if copy-pasted Rust code in src/ exceeds the threshold in .jscpd.json.

	The threshold is the duplication measured when the gate was added, so it
	passes today and can only go down: lower it whenever a refactor removes
	duplicates, never raise it. Run "npx jscpd --reporters console" to list the
	clones.
	EOF
}

[[ "${1:-}" != "--help" ]] || {
  print_help
  exit 0
}

npx --no-install jscpd --config .jscpd.json
