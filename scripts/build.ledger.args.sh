#!/usr/bin/env bash
set -euo pipefail

# This script builds the init arguments for the Virtual USD Ledger canister,
# taking into account the target DFX network (ic, staging, local).
# It considers whether the canister is already installed to determine
# whether to use the Init or Upgrade variant for the arguments.
# The interface of the Init variant can be found here:
# https://github.com/dfinity/ic/blob/d5b336cf169b3fec81385701a23e92388e8f77ae/rs/ledger_suite/icrc1/ledger/src/lib.rs#L270

ECHO "Building Ledger args..."

MODE="${1:-auto}"
case "$MODE" in
auto | init | upgrade) ;;
*)
  ECHO "Usage: $0 [auto|init|upgrade]"
  ECHO "       mode: auto (default), init, upgrade"
  exit 1
  ;;
esac

DFX_NETWORK="${DFX_NETWORK:-local}"
ECHO "Building Ledger args for network: ${DFX_NETWORK}"

if ! dfx ping "$DFX_NETWORK" >/dev/null 2>&1; then
  echo "ERROR: Unknown DFX network '${DFX_NETWORK:-<unset>}'"
  exit 1
fi

TOKEN_SYMBOL="vUSD"
TOKEN_NAME="Virtual USD"
LOGO_FILE="assets/logo/vUSD.svg"

TRANSFER_FEE=100_000
DECIMALS=8

# Portable base64 (works on Linux and macOS)
encode_b64() {
  if base64 --help 2>&1 | grep -q -- '-w '; then
    base64 -w 0 "$1"
  else
    base64 <"$1" | tr -d '\n'
  fi
}

MIME_TYPE="image/png"

B64_LOGO="$(encode_b64 "$LOGO_FILE")"
DATA_URI="data:${MIME_TYPE};base64,${B64_LOGO}"

PRINCIPAL="$(dfx identity get-principal)"
MINTER_CANISTER_ID="$(dfx canister id minter 2>/dev/null || true)"
MINTER_PRINCIPAL="${MINTER_PRINCIPAL:-${MINTER_CANISTER_ID:-$PRINCIPAL}}"
echo "Using minter principal: $MINTER_PRINCIPAL"

if [[ "$MODE" == "upgrade" ]]; then
  VARIANT="Upgrade"
elif [[ "$MODE" == "init" ]]; then
  VARIANT="Init"
else
  if scripts/check.canister.installed.sh ledger "$DFX_NETWORK"; then
    VARIANT="Upgrade"
  else
    VARIANT="Init"
  fi
fi

ARG_FILE="$(jq -re .canisters.ledger.init_arg_file dfx.json)"

mkdir -p "$(dirname "$ARG_FILE")"

if [[ "$VARIANT" == "Upgrade" ]]; then

  # Use Upgrade variant: same values, but everything is opt
  cat <<-EOF >"$ARG_FILE"
  (
    variant {
      Upgrade = opt record {
        token_symbol = opt "$TOKEN_SYMBOL";
        token_name = opt "$TOKEN_NAME";
        transfer_fee = opt $TRANSFER_FEE;
        decimals = opt $DECIMALS;
        metadata = opt vec {
          record {
            "icrc1:logo"; variant { Text = "$DATA_URI" }
          }
        };
        feature_flags = opt record {
          icrc2 = true
        }
      }
    }
  )
EOF

else

  # Original Init variant
  cat <<-EOF >"$ARG_FILE"
  (
    variant {
      Init = record {
        token_symbol = "$TOKEN_SYMBOL";
        token_name = "$TOKEN_NAME";
        transfer_fee = $TRANSFER_FEE;
        decimals = opt $DECIMALS;
        metadata = vec {
          record {
            "icrc1:logo"; variant {
              Text = "$DATA_URI"
            }
          }
        };
        feature_flags = opt record {
          icrc2 = true
        };
        minting_account = record {
          owner = principal "$MINTER_PRINCIPAL"
        };
        initial_balances = vec {};
        archive_options = record {
          num_blocks_to_archive = 1_000;
          trigger_threshold = 2_000;
          controller_id = principal "$PRINCIPAL";
          cycles_for_archive_creation = opt 10_000_000_000_000
        }
      }
    }
  )
EOF

fi
