.DEFAULT_GOAL := help

.PHONY: help build update-deps test check format fmt-check lint architecture docs quality quality-baseline audit hooks cache-stats clean publish-check publish publish-dry-run

help: ## Show available targets
	@printf "Available targets:\n"
	@awk 'BEGIN {FS = ":.*## "}; /^[a-zA-Z0-9_.-]+:.*## / {printf "  %-16s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: ## Build every crate
	mise run build

update-deps: ## Update Cargo.lock to the latest compatible dependencies
	mise run update-deps

test: ## Run the tests
	mise run test

check: ## Architecture, docs, format check, lint, tests and packaging
	mise run check

publish-check: ## Before a release: verify every package again online, against the live crates.io index
	mise run publish-check

publish-dry-run: ## Everything a publish does except the upload
	mise exec -- bash scripts/publish.sh --dry-run

publish: ## Publish every crate to crates.io (needs CARGO_REGISTRY_TOKEN; see docs/publishing.md)
	mise exec -- bash scripts/publish.sh

format: ## Format the Rust sources
	mise run fmt

fmt-check: ## Check the formatting
	mise run fmt-check

lint: ## Run cargo-shear and clippy with warnings denied
	mise run lint

architecture: ## Check layer directions and source rules
	mise run architecture

docs: ## Check the documentation index and the Space guides
	mise run docs-index board-docs

quality: ## Fail when the rustqual score drops below the baseline
	mise run quality

quality-baseline: ## Save the current rustqual results as the baseline
	mise run quality-baseline

audit: ## Check the lockfile against the RustSec advisory database (network)
	mise run audit

hooks: ## Install the prek commit and push hooks
	prek install

cache-stats: ## Show Mr Boxington cache results for the last build
	mise run cache-stats

clean: ## Remove build output
	mise run clean
