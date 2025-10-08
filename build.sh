#!/usr/bin/env bash
set -euo pipefail

TARGET="wasm32-unknown-unknown"
CANISTER="liquidium_staking"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &> /dev/null && pwd)"
FEATURES="${1:-}"

pushd "$SCRIPT_DIR" > /dev/null

echo "Running build process.."

# macOS requires specific llvm tools for wasm build (esp. rust-secp256k1)
if [[ "$(uname)" == "Darwin" ]]; then
  LLVM_PATH="$(brew --prefix llvm)"
  AR="${LLVM_PATH}/bin/llvm-ar" \
  CC="${LLVM_PATH}/bin/clang" \
  cargo build --target "$TARGET" --release --features "$FEATURES"
else
  cargo build --target "$TARGET" --release --features "$FEATURES"
fi

# Extract Candid interface
candid-extractor "target/${TARGET}/release/${CANISTER}.wasm" > "${CANISTER}.did"

# Install ic-wasm locally (if not already available)
cargo install ic-wasm --version 0.2.0 --root ./ || true

# Inject Candid metadata
./bin/ic-wasm \
  "target/${TARGET}/release/${CANISTER}.wasm" \
  -o "target/${TARGET}/release/${CANISTER}.wasm" \
  metadata candid:service -f "${SCRIPT_DIR}/${CANISTER}.did" -v public

popd > /dev/null
