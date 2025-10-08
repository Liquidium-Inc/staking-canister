#!/bin/bash

echo "POOL DATA INITIALIZATION"

# Set the canister ID
CANISTER_ID="34r2u-nqaaa-aaaaj-qnova-cai"

echo "Initializing pool data (address, xpub, fingerprint) for indices 0-1..."

# Call the canister's initialize_pool_addresses_range function
echo "Calling initialize_pool_addresses_range(0, 1)..."
RESPONSE=$(dfx canister --network ic call "$CANISTER_ID" initialize_pool_addresses_range '(0 : nat32, 1 : nat32)')

echo "Response:"
echo "$RESPONSE"

if [[ "$RESPONSE" == *"Error"* ]]; then
    echo "ERROR: Pool initialization failed"
    exit 1
else
    echo "SUCCESS: Pool data initialized"
fi

echo "Pool initialization complete"
