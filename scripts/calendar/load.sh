#!/usr/bin/env bash
set -euo pipefail

print_help() {
  cat <<-EOF

	Loads a calendar built by scripts/calendar/build.mjs into the clearing canister.

	Dry run by default: reads the events already on the canister and prints what would be
	added. Nothing is written unless --apply is given. Events already present (same time and
	label) are skipped, so the script can be rerun safely. At most 512 events are kept per
	underlying, as the canister enforces.

	Usage:
	  $0 --file calendar.json [--network local|ic] [--identity NAME] [--canister clearing] [--apply]

	The identity must hold the risk administrator role (add_calendar_events), and each
	underlying must already be configured with set_underlying_risk_params.

	EOF
}

FILE=""
NETWORK="local"
IDENTITY=""
CANISTER="clearing"
APPLY="false"
MAX_EVENTS=512
CHUNK=50

while [[ $# -gt 0 ]]; do
  case "$1" in
  --file)
    FILE="$2"
    shift 2
    ;;
  --network)
    NETWORK="$2"
    shift 2
    ;;
  --identity)
    IDENTITY="$2"
    shift 2
    ;;
  --canister)
    CANISTER="$2"
    shift 2
    ;;
  --apply)
    APPLY="true"
    shift
    ;;
  --help | -h)
    print_help
    exit 0
    ;;
  *)
    echo "Unknown argument: $1" >&2
    print_help >&2
    exit 1
    ;;
  esac
done

[[ -n "$FILE" && -f "$FILE" ]] || {
  echo "ERROR: --file must point to a calendar built by scripts/calendar/build.mjs" >&2
  exit 1
}

DFX_ARGS=(--network "$NETWORK")
[[ -z "$IDENTITY" ]] || DFX_ARGS+=(--identity "$IDENTITY")

# Builds the candid argument of add_calendar_events from a JSON array of events.
candid_args() {
  local underlying="$1"
  jq -r --arg u "$underlying" '
    def text: gsub("\\\\"; "\\\\\\\\") | gsub("\""; "\\\"");
    "(record { underlying = \"" + ($u | text) + "\"; events = vec { "
    + ([.[] | "record { time_ns = " + (.time_ns | tostring) + " : nat64; weight = record { decimals = 2 : nat8; value = "
      + ((.weight * 100) | round | tostring) + " : nat }; label = \"" + (.label | text) + "\" }"] | join("; "))
    + " } })"'
}

FAILED=0

while IFS= read -r UNDERLYING; do
  EVENTS="$(jq -c --arg u "$UNDERLYING" '.calendars[$u]' "$FILE")"

  if EXISTING="$(dfx canister call "${DFX_ARGS[@]}" --query --output json "$CANISTER" list_calendar_events "(\"$UNDERLYING\")" 2>/dev/null)"; then
    :
  elif [[ "$APPLY" == "true" ]]; then
    echo "ERROR   $UNDERLYING: cannot read existing events from $CANISTER on $NETWORK" >&2
    FAILED=1
    continue
  else
    echo "WARN    $UNDERLYING: cannot read existing events, showing every event as new" >&2
    EXISTING="[]"
  fi

  # New events: those whose (time, label) is not already on the canister.
  NEW="$(jq -c --argjson existing "$EXISTING" '
    ($existing | map((.time_ns | tostring) + "|" + .label)) as $have
    | map(select(((.time_ns | tostring) + "|" + .label) as $k | $have | index($k) | not))' <<<"$EVENTS")"
  HAVE="$(jq 'length' <<<"$EXISTING")"
  WANT="$(jq 'length' <<<"$NEW")"
  ROOM=$((MAX_EVENTS - HAVE))
  ((ROOM > 0)) || ROOM=0
  if ((WANT > ROOM)); then
    echo "WARN    $UNDERLYING: $WANT new events but room for $ROOM; keeping the earliest $ROOM" >&2
    NEW="$(jq -c --argjson n "$ROOM" '.[:$n]' <<<"$NEW")"
    WANT="$ROOM"
  fi

  echo "$UNDERLYING: $HAVE on canister, $WANT to add"
  jq -r '.[] | "  + \(.time_utc)  w=\(.weight)  \(.label)  (\(.source_url))"' <<<"$NEW"

  if [[ "$APPLY" == "true" && "$WANT" -gt 0 ]]; then
    for ((i = 0; i < WANT; i += CHUNK)); do
      ARGS="$(jq -c --argjson i "$i" --argjson n "$CHUNK" '.[$i:$i + $n]' <<<"$NEW" | candid_args "$UNDERLYING")"
      if ! RESULT="$(dfx canister call "${DFX_ARGS[@]}" --output json "$CANISTER" add_calendar_events "$ARGS" 2>&1)"; then
        echo "ERROR   $UNDERLYING: call rejected: $RESULT" >&2
        FAILED=1
        break
      fi
      if jq -e 'has("Ok")' <<<"$RESULT" >/dev/null 2>&1; then
        echo "  added $(jq '.Ok | length' <<<"$RESULT") events"
      else
        echo "ERROR   $UNDERLYING: $RESULT" >&2
        FAILED=1
        break
      fi
    done
  fi
done < <(jq -r '.calendars | keys[]' "$FILE")

if [[ "$APPLY" != "true" ]]; then
  echo
  echo "Dry run: nothing was written. Rerun with --apply to load these events."
fi

exit "$FAILED"
