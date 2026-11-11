#!/usr/bin/env bash
set -euo pipefail

print_help() {
  cat <<-EOF

	Loads a calendar built by scripts/calendar/build.mjs into the clearing canister.

	Dry run by default: reads the events already on the canister and prints what would be
	added. Nothing is written unless --apply is given. Events already present (same time and
	label) are skipped, so the script can be rerun safely; to change the weight of an existing
	event, remove it first with remove_calendar_event. Events less than 10 minutes away are not
	sent, and if the canister rejects a batch for a past or duplicate event the script re-reads
	the calendar and retries once. At most 512 live events are kept per underlying, as the
	canister enforces (events that ended more than a day ago are pruned and do not count).

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

NS_PER_SEC=1000000000
# Events closer than this to now are not sent: the calendar file may be older than the load,
# and the canister rejects the whole batch if one event is already in the past.
MIN_LEAD_NS=$((600 * NS_PER_SEC))
NS_PER_DAY=$((86400 * NS_PER_SEC))

now_ns() {
  echo $(($(date +%s) * NS_PER_SEC))
}

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

# Reads the events already on the canister into EXISTING.
# Returns 0 on success, 2 if the underlying has no risk parameters yet, 1 on any other failure.
read_existing() {
  local underlying="$1" result
  if ! result="$(dfx canister call "${DFX_ARGS[@]}" --query --output json "$CANISTER" list_calendar_events "(record { underlying = \"$underlying\" })" 2>&1)"; then
    EXISTING_ERROR="$result"
    return 1
  fi
  if jq -e 'has("Ok")' <<<"$result" >/dev/null 2>&1; then
    EXISTING="$(jq -c '.Ok' <<<"$result")"
    return 0
  fi
  EXISTING_ERROR="$result"
  if jq -e '.Err | has("UnknownUnderlying")' <<<"$result" >/dev/null 2>&1; then
    return 2
  fi
  return 1
}

# Prints, as a JSON array, the events of $1 that are neither on the canister ($2, same time and
# label) nor closer to now than MIN_LEAD_NS.
filter_new() {
  local events="$1" existing="$2" cutoff
  cutoff=$(($(now_ns) + MIN_LEAD_NS))
  jq -c --argjson existing "$existing" --arg cutoff "$cutoff" '
    ($existing | map((.time_ns | tostring) + "|" + .label)) as $have
    | map(select((.time_ns | tonumber) > ($cutoff | tonumber)))
    | map(select(((.time_ns | tostring) + "|" + .label) as $k | $have | index($k) | not))' <<<"$events"
}

# Sends one batch. On a rejection for a past or duplicate event (the calendar changed between
# the read and the write), re-reads the canister, drops what is no longer new and retries once.
send_batch() {
  local underlying="$1" batch="$2" attempt result args
  for attempt in 1 2; do
    [[ "$(jq 'length' <<<"$batch")" -gt 0 ]] || {
      echo "  nothing left to add in this batch"
      return 0
    }
    args="$(candid_args "$underlying" <<<"$batch")"
    if ! result="$(dfx canister call "${DFX_ARGS[@]}" --output json "$CANISTER" add_calendar_events "$args" 2>&1)"; then
      echo "ERROR   $underlying: call rejected: $result" >&2
      return 1
    fi
    if jq -e 'has("Ok")' <<<"$result" >/dev/null 2>&1; then
      echo "  added $(jq '.Ok | length' <<<"$result") events"
      return 0
    fi
    if [[ "$attempt" == 1 ]] && jq -e '.Err.InvalidParameter.reason | test("in the past|duplicate")' <<<"$result" >/dev/null 2>&1; then
      echo "  canister reports a past or duplicate event; re-reading and retrying once"
      read_existing "$underlying" || {
        echo "ERROR   $underlying: cannot re-read existing events: $EXISTING_ERROR" >&2
        return 1
      }
      batch="$(filter_new "$batch" "$EXISTING")"
      continue
    fi
    echo "ERROR   $underlying: $result" >&2
    return 1
  done
}

FAILED=0

while IFS= read -r UNDERLYING; do
  EVENTS="$(jq -c --arg u "$UNDERLYING" '.calendars[$u]' "$FILE")"

  EXISTING="[]"
  EXISTING_ERROR=""
  set +e
  read_existing "$UNDERLYING"
  STATUS=$?
  set -e
  if ((STATUS == 2)); then
    if [[ "$APPLY" == "true" ]]; then
      echo "ERROR   $UNDERLYING: not configured on $CANISTER; run set_underlying_risk_params first" >&2
      FAILED=1
      continue
    fi
    echo "WARN    $UNDERLYING: not configured on $CANISTER yet (set_underlying_risk_params); showing every event as new" >&2
    EXISTING="[]"
  elif ((STATUS != 0)); then
    if [[ "$APPLY" == "true" ]]; then
      echo "ERROR   $UNDERLYING: cannot read existing events from $CANISTER on $NETWORK: $EXISTING_ERROR" >&2
      FAILED=1
      continue
    fi
    echo "WARN    $UNDERLYING: cannot read existing events, showing every event as new" >&2
    EXISTING="[]"
  fi

  NEW="$(filter_new "$EVENTS" "$EXISTING")"
  # The canister prunes events that ended more than a day ago before counting the cap.
  HAVE="$(jq --arg floor "$(($(now_ns) - NS_PER_DAY))" '[.[] | select((.time_ns | tonumber) >= ($floor | tonumber))] | length' <<<"$EXISTING")"
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
      BATCH="$(jq -c --argjson i "$i" --argjson n "$CHUNK" '.[$i:$i + $n]' <<<"$NEW")"
      send_batch "$UNDERLYING" "$BATCH" || {
        FAILED=1
        break
      }
    done
  fi
done < <(jq -r '.calendars | keys[]' "$FILE")

if [[ "$APPLY" != "true" ]]; then
  echo
  echo "Dry run: nothing was written. Rerun with --apply to load these events."
fi

exit "$FAILED"
