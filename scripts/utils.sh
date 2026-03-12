#!/usr/bin/env bash

set -euo pipefail

# Common utility functions for scripts

# Default network
NETWORK="local"

# Acceptable networks
ACCEPTABLE_NETWORKS=("local" "staging")

# Function to check if the network is acceptable
check_network() {
  local requested_network=$1

  # Remove leading dashes if any
  requested_network=${requested_network#--}

  if [[ -z "$requested_network" ]]; then
    return 0
  fi

  for net in "${ACCEPTABLE_NETWORKS[@]}"; do
    if [[ "$net" == "$requested_network" ]]; then
      NETWORK="$requested_network"
      return 0
    fi
  done

  echo "Error: Network '$requested_network' is not supported. Acceptable networks are: ${ACCEPTABLE_NETWORKS[*]}"
  exit 1
}

# Parse basic arguments to find network
while [[ $# -gt 0 ]]; do
  case $1 in
  local | staging)
    NETWORK="$1"
    shift
    ;;
  --local)
    NETWORK="local"
    shift
    ;;
  --staging)
    NETWORK="staging"
    shift
    ;;
  *)
    # Check if it's an invalid network argument
    if [[ "$1" == --* ]]; then
      check_network "${1#--}"
    fi
    shift
    ;;
  esac
done

# Final validation (in case it was set directly but incorrectly, or not caught by loop)
check_network "$NETWORK"

export NETWORK
echo "Using network: $NETWORK"
