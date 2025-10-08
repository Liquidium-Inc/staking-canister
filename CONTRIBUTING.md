# Contributing Guidelines

Thank you for your interest in contributing to the Liquidium Staking canister! This document explains how to set up a development environment, the standards we follow, and how to propose changes.

## Development Environment

- **Rust**: Install the latest stable toolchain via [`rustup`](https://rustup.rs). The repository includes a `rust-toolchain.toml` pinning the toolchain.
- **dfx**: Install the Internet Computer SDK (`dfx`) 0.20 or newer. See the [dfx docs](https://internetcomputer.org/docs/current/references/cli-reference/dfx-toolchain) for installation instructions.
- **Wasmtime** (optional): Some tests may use Wasmtime for local execution.
- **Utilities**: GNU Make and jq are used by helper scripts.

After cloning your fork:

```bash
cargo fmt
cargo test
make build
```

Run `make deploy` to deploy to a local replica or `make deploy-mainnet` for production (requires configured identities and cycles).

## Workflow

1. **Fork the repository** and branch from `main`.
2. **Keep changes scoped.** Separate unrelated fixes into distinct pull requests.
3. **Add or update tests** (`cargo test`, property tests, integration scripts) for critical logic.
4. **Document behavior changes** in the README or inline doc comments.
5. **Open a pull request** describing the problem, solution, and any follow-up steps.

## Coding Standards

- Follow Rust's standard style enforced by `cargo fmt` and `cargo clippy`.
- Prefer explicit types and error handling via `Result`.
- Keep canister interfaces stable; document any breaking changes in the changelog or release notes.
- Avoid committing generated artifacts (`target/`, `.dfx/`, etc.).

Run these commands before submitting a PR:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Commit & PR Guidelines

- Use descriptive commit messages (e.g., `feat: expose staking metrics query`).
- Ensure CI checks pass before requesting review.
- Provide context for reviewers, including design considerations and testing performed.

## Communication

- Use GitHub issues or discussions for feature proposals and architectural questions.
- Mention maintainers (e.g., `@Liquidium-Inc/staking-canister-maintainers`) in your PR for reviews when ready.

By contributing, you agree to our [Code of Conduct](./CODE_OF_CONDUCT.md) if present, or the organization-wide policy.

We appreciate your help in strengthening the Liquidium staking canister!
