FEATURES ?=

.PHONY: all
all: build

.PHONY: build
.SILENT: build
build:
	@echo "Building for FEATURES $(FEATURES)..."
	./build.sh "$(FEATURES)"


# Deploy dev
.PHONY: deploy-dev
.SILENT: deploy-dev
deploy-dev:
	$(MAKE) build FEATURES=dev-hooks
	@echo "Deploying liquidium_staking dev to fiduciary..."
	dfx deploy --network=ic liquidium_staking --subnet-type fiduciary

# Deploy prod
.PHONY: deploy-prod
.SILENT: deploy-prod
deploy-prod:
	$(MAKE) build FEATURES=prod
	@echo "Deploying liquidium_staking_prod to fiduciary..."
	dfx deploy --network=ic liquidium_staking_prod --subnet-type fiduciary
# 	dfx-orbit request canister install liquidium_staking_prod --mode upgrade --wasm ./target/wasm32-unknown-unknown/release/liquidium_staking.wasm --asset-canister incev-lqaaa-aaaap-qpwva-cai

# Default deploy is to local network
.PHONY: deploy
.SILENT: deploy
deploy: deploy-local


# Check ICP balance on mainnet
.PHONY: check_icp_balance
.SILENT: check_icp_balance
check_icp_balance:
	@echo "Checking ICP balance on mainnet..."
	@dfx ledger --network=ic balance

# Shorthand for check_icp_balance
.PHONY: balance
.SILENT: balance
balance: check_icp_balance


# Delete all build artifacts
.PHONY: clean
.SILENT: clean
clean:
	rm -rf .dfx
	rm -rf dist
	rm -rf node_modules
	rm -rf src/declarations
	rm -f .env
	cargo clean
