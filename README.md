# Liquidium Staking Protocol

A Bitcoin staking protocol that allows users to stake Bitcoin and receive liquid staking tokens.

## Prerequisites

- Rust (stable) via `rustup`
- Internet Computer SDK (`dfx`) 0.20 or newer
- GNU Make
- (Optional) Wasmtime for local execution tests

## Building and Deploying

### Build the canister
```bash
make build
```

### Deploy options
```bash
# Deploy to local network (default)
make deploy

# Deploy to IC mainnet with Bitcoin mainnet
make deploy-mainnet
```

### Other commands
```bash
# Check ICP balance on mainnet
make balance

# Clean build artifacts
make clean
```

## Testing

### Run Rust unit tests
```bash
cargo test
```

### Run integration test scripts
```bash
# Navigate to test directory
cd test

# Run individual test scripts
./stake.sh
./unstake.sh
./withdraw.sh
./get_pool_info.sh
./get_exchange_rate.sh
./get_recent_unstake_records.sh
```

## Pool Initialization

When deployed for the first time, pool is automatically initialized.
But if the canister is already deployed, the previous version, we need to initialize the pool addresses manually:

```bash
./init_pool.sh
```

This generates Bitcoin addresses for the staking pools (indices 0-1).

---

## Audit

The Liquidium Staking canister was independently audited by ScaleBit. You can review the full report here: [Liquidium Staking Audit (September 1, 2025)](https://stake.liquidium.org/audit.pdf).

---

## Contributing

Interested in contributing? Please review [CONTRIBUTING.md](./CONTRIBUTING.md) for setup instructions, coding standards, and pull request guidelines. All contributors must adhere to our [Code of Conduct](./CODE_OF_CONDUCT.md).

## Security

If you discover a security vulnerability, follow the responsible disclosure process outlined in [SECURITY.md](./SECURITY.md). Kindly avoid filing public issues for security reports.

## License

Distributed under the [GNU GPL v3.0](./LICENSE).
