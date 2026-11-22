# Repository entry point. Plain `make` lists the targets. The targets below
# run in both projects, the Framework first; every other target runs in
# subspace-desktop/, so `make run` or `make release-macos SIGNING_IDENTITY=...`
# work from the repository root. Each project works without this file.

.DEFAULT_GOAL := help
FRAMEWORK := subspace-framework
DESKTOP := subspace-desktop
BOTH := build test check format fmt-check lint quality audit

STACK := bash scripts/local-stack.sh

.PHONY: help hooks release-check $(BOTH) stack-up stack-down stack-logs stack-reset stack-demo stack-seed stack-desktop-config stack-check dev-token

help: ## Show available targets
	@printf "Repository targets (run in %s/, then %s/):\n" "$(FRAMEWORK)" "$(DESKTOP)"
	@printf "  %-16s %s\n" $(foreach target,$(BOTH),"$(target)" "both projects")
	@awk 'BEGIN {FS = ":.*## "}; /^[a-zA-Z0-9_.-]+:.*## / {printf "  %-16s %s\n", $$1, $$2}' $(firstword $(MAKEFILE_LIST))
	@printf "\nSubspace Desktop targets (run in %s/):\n" "$(DESKTOP)"
	@$(MAKE) --no-print-directory -C $(DESKTOP) help | tail -n +2 | grep -v "^  help "

hooks: ## Install the prek commit and push hooks for both projects
	prek install

release-check: ## Release Please's config, the lockfile sync, the Stargate pin tooling and the release workflows (offline)
	@cmp -s LICENSE $(FRAMEWORK)/LICENSE || { echo "The Framework's LICENSE must match the repository LICENSE" >&2; exit 1; }
	mise -C $(DESKTOP) exec -- python3 "$(CURDIR)/scripts/test-release-tooling.py"
	bash scripts/test-release-lockfiles.sh
	bash scripts/test-release-rebuild.sh
	bash scripts/test-stargate-bump.sh
	python3 scripts/check-release-config.py
	mise -C $(DESKTOP) exec -- python3 "$(CURDIR)/scripts/sync-lockfiles.py" --check
	@if command -v actionlint >/dev/null; then actionlint .github/workflows/*.yml; else echo "actionlint is not installed; workflows not linted"; fi

ifneq ($(origin DEMO),undefined)
stack-up: export SUBSPACE_STACK_DEMO_DATA = $(DEMO)
endif
stack-up: ## Start the local stack and configure Channels (DEMO=0 for an empty workspace)
	$(STACK) up

stack-down: ## Stop the local stack and keep its data
	docker compose down

stack-logs: ## Follow the local stack's logs
	docker compose logs --follow

stack-reset: ## Remove the local stack's containers, data and generated passwords
	$(STACK) reset

stack-demo: ## Add or top up demo data and IRC history on the running stack
	$(STACK) demo

stack-seed: stack-demo ## Alias for stack-demo

stack-desktop-config: ## Install the local IRC networks using the generated passwords
	@$(STACK) stack-desktop-config

stack-check: ## Check the stack commands (STACK_LIVE=1 also checks running services)
	bash scripts/test-local-stack.sh
	@if [ "$(STACK_LIVE)" = 1 ]; then bash scripts/test-local-stack-live.sh; fi

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
