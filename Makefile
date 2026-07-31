# Internal repository gates. User-facing commands live in README.md.

.DEFAULT_GOAL := help
.PHONY: help pre-commit-checks test ci

help: ## Show this help.
	@awk 'BEGIN{FS=":.*##"; printf "Usage: make \033[36m<target>\033[0m\n\nTargets:\n"} \
	      /^[a-zA-Z0-9_-]+:.*?##/{ printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2 }' \
	    $(MAKEFILE_LIST)

pre-commit-checks: ## Run formatting and strict lint checks.
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo fmt --manifest-path demo/Cargo.toml -- --check
	cargo clippy --manifest-path demo/Cargo.toml --all-targets -- -D warnings

test: ## Run the full Rust test suite.
	cargo test --all-features

ci: ## Run the portable pull-request gate.
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo fmt --manifest-path demo/Cargo.toml -- --check
	cargo clippy --manifest-path demo/Cargo.toml --all-targets -- -D warnings
	cargo test --all-features
