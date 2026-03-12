#!/usr/bin/env bash

source "$(dirname "$0")/utils.sh" "$@"

echo "Reinstalling clearing on network: $NETWORK"
dfx deploy clearing --network "$NETWORK" --upgrade-unchanged --mode reinstall --yes

echo "Reinstalling registry on network: $NETWORK"
dfx deploy registry --network "$NETWORK" --upgrade-unchanged --mode reinstall --yes

echo "Reinstallation complete."
