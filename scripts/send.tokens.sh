#!/bin/bash

# Utility Script: Send Tokens to a Local User
# Particularly handy after installing the canisters for the first time or being forced by dfx to clean the local state.

if [ -z "$1" ]; then
  read -r -p "Enter the PRINCIPAL: " PRINCIPAL
else
  PRINCIPAL=$1
fi

if [ -z "$2" ]; then
  AMOUNT_E8S=100_000_000 # 1 ICP
else
  AMOUNT_E8S=$(echo "$2 * 100000000" | bc)
fi

DFX_NETWORK=local

dfx canister call icp_ledger --network "$DFX_NETWORK" icrc1_transfer "(record {from=null; to=record { owner= principal \"$PRINCIPAL\";}; amount=$AMOUNT_E8S; fee=null; memo=null; created_at_time=null;})"
