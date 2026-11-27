#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$(realpath "$0")")/.." || exit

print_help() {
  cat <<EOF

Fails if any tracked file contains an em dash (U+2014) or an en dash (U+2013).
Rewrite the sentence with a colon, comma, parentheses or a period instead;
write a range with 'to' or a hyphen.

EOF
}

[[ "${1:-}" != "--help" ]] || {
  print_help
  exit 0
}

# Built from their UTF-8 bytes so this script never contains the characters themselves.
em_dash=$(printf '\xe2\x80\x94')
en_dash=$(printf '\xe2\x80\x93')

set +e
matches=$(git grep -I -n -F -e "$em_dash" -e "$en_dash")
status=$?
set -e

case $status in
0)
  echo "ERROR: em dashes (U+2014) and en dashes (U+2013) are not allowed in this repository:"
  echo "$matches"
  echo "Rewrite each sentence with a colon, comma, parentheses or a period; write a range with 'to' or a hyphen."
  exit 1
  ;;
1) ;;
*)
  echo "ERROR: git grep failed with status $status."
  exit "$status"
  ;;
esac
