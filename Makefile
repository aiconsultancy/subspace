# Repository entry point. Plain `make` lists the targets. The targets below
# run in both projects, the Framework first; every other target runs in
# subspace-desktop/, so `make run` or `make release-macos SIGNING_IDENTITY=...`
# work from the repository root. Each project works without this file.

.DEFAULT_GOAL := help
FRAMEWORK := subspace-framework
DESKTOP := subspace-desktop
BOTH := build test check format fmt-check lint quality audit

STACK := bash scripts/local-stack.sh

.PHONY: help hooks release-check $(BOTH) stack-up stack-down stack-logs stack-reset stack-seed stack-desktop-config stack-check dev-token

help: ## Show available targets
	@printf "Repository targets (run in %s/, then %s/):\n" "$(FRAMEWORK)" "$(DESKTOP)"
	@printf "  %-16s %s\n" $(foreach target,$(BOTH),"$(target)" "both projects")
	@awk 'BEGIN {FS = ":.*## "}; /^[a-zA-Z0-9_.-]+:.*## / {printf "  %-16s %s\n", $$1, $$2}' $(firstword $(MAKEFILE_LIST))
	@printf "\nSubspace Desktop targets (run in %s/):\n" "$(DESKTOP)"
	@$(MAKE) --no-print-directory -C $(DESKTOP) help | tail -n +2 | grep -v "^  help "

hooks: ## Install the prek commit and push hooks for both projects
	prek install

release-check: ## Release Please's config, the lockfile sync and the release workflows
	mise -C $(DESKTOP) exec -- python3 "$(CURDIR)/scripts/test-release-tooling.py"
	bash scripts/test-release-lockfiles.sh
	bash scripts/test-release-rebuild.sh
	python3 scripts/check-release-config.py
	mise -C $(DESKTOP) exec -- python3 "$(CURDIR)/scripts/sync-lockfiles.py" --check
	@if command -v actionlint >/dev/null; then actionlint .github/workflows/*.yml; else echo "actionlint is not installed; workflows not linted"; fi

stack-up: ## Start the local stack, seed demo data and configure Channels
	$(STACK) up

stack-down: ## Stop the local stack and keep its data
	docker compose down

stack-logs: ## Follow the local stack's logs
	docker compose logs --follow

stack-reset: ## Remove the local stack's containers, data and generated passwords
	$(STACK) reset

stack-seed: ## Seed the local stack's demo data again
	$(STACK) seed

stack-desktop-config: ## Install the local IRC networks using the generated passwords
	@$(STACK) stack-desktop-config

stack-check: ## Check the local stack script against fixtures
	bash scripts/test-local-stack.sh

dev-token: ## Print a development bearer token for the local API
	@$(STACK) token

$(BOTH):
	@$(MAKE) --no-print-directory -C $(FRAMEWORK) $@
	@$(MAKE) --no-print-directory -C $(DESKTOP) $@

check: release-check stack-check

# The makefile itself is not forwarded.
Makefile: ;

%:
	@$(MAKE) --no-print-directory -C $(DESKTOP) $@
