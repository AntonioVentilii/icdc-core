#!/usr/bin/env bash
set -euo pipefail

print_help() {
  cat <<-EOF

	Checks that each canister's Candid interface is still backward-compatible
	with a baseline (by default the PR's base branch).

	For every local candid file declared in dfx.json (\`.canisters[].candid\`
	under \`src/\`), the working-tree interface is compared against the same file
	at the baseline commit using \`didc check <new> <old>\`, which succeeds only
	when <new> is a Candid subtype of <old>. Compatible evolutions (adding a new
	method, adding an optional record field, …) pass; removing a method or
	changing an existing signature fails.

	A failure means the change is BREAKING: the PR must be marked as such with a
	Conventional Commits \`!\` in the title AND a \`BREAKING CHANGE:\` footer in the
	body, after which CI stops requiring compatibility for that PR.

	Usage:
	  scripts/check.did.compat.sh [BASELINE]

	  BASELINE   Git revision to compare against (a ref, branch, or commit SHA).
	             Defaults to \$BASE_REF, then to \`origin/main\`.

	EOF
}

[[ "${1:-}" != "--help" ]] || {
  print_help
  exit 0
}

BASELINE="${1:-${BASE_REF:-origin/main}}"

command -v didc >/dev/null 2>&1 || {
  echo "error: didc not found in PATH." >&2
  echo "  From the repository root, run: scripts/setup didc" >&2
  echo "  (installs the version pinned in dev-tools.json; ensure ~/.local/bin is on PATH)" >&2
  exit 1
}

# Candid interfaces we ship, taken from dfx.json and restricted to source-tree
# files (excludes downloaded/generated interfaces such as the ledgers under
# target/).
CANDID_FILES=()
while IFS= read -r candid; do
  [[ -n "$candid" ]] && CANDID_FILES+=("$candid")
done < <(jq -r '.canisters | to_entries[] | .value.candid? // empty | select(startswith("src/"))' dfx.json)

((${#CANDID_FILES[@]})) || {
  echo "ERROR: No source candid files found in dfx.json (.canisters[].candid under src/)."
  exit 1
}

echo "Comparing candid interfaces against baseline: $BASELINE"

incompatible=()
checked=0
for candid in "${CANDID_FILES[@]}"; do
  if [[ ! -f "$candid" ]]; then
    echo "  - $candid: skipped (absent in working tree)"
    continue
  fi

  # A canister that does not exist at the baseline is brand new: there is no
  # prior interface to break, so nothing to check.
  if ! git cat-file -e "$BASELINE:$candid" 2>/dev/null; then
    echo "  - $candid: skipped (new interface, absent at baseline)"
    continue
  fi

  old_did="$(mktemp)"
  # shellcheck disable=SC2064
  trap "rm -f '$old_did'" EXIT
  git show "$BASELINE:$candid" >"$old_did"

  checked=$((checked + 1))
  if didc check "$candid" "$old_did"; then
    echo "  - $candid: compatible"
  else
    echo "  - $candid: INCOMPATIBLE (not a subtype of the baseline interface)"
    incompatible+=("$candid")
  fi
  rm -f "$old_did"
  trap - EXIT
done

echo "Checked $checked candid interface(s)."

if ((${#incompatible[@]})); then
  {
    echo
    echo "ERROR: The following candid interfaces are NOT backward-compatible with the baseline:"
    printf '  - %s\n' "${incompatible[@]}"
    cat <<-EOF
	This is a BREAKING change. If it is intentional, declare it on the PR using the
	Conventional Commits breaking-change convention, then push another commit:
	  - title: add a '!' before the colon, e.g. 'feat(clearing)!: change the settlement API'
	  - body:  add a footer line, e.g. 'BREAKING CHANGE: the settlement API is now incompatible'
	Once BOTH markers are present, this check no longer requires compatibility for the PR.
	EOF
  } >&2
  exit 1
fi

echo "All candid interfaces are backward-compatible."
