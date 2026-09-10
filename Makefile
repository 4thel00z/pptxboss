# Self-documenting Makefile: `make` or `make help` lists the targets.
.DEFAULT_GOAL := help
SHELL := /bin/bash

##@ Rust

build: ## Build every crate (debug)
	cargo build --workspace

release: ## Build every crate (release)
	cargo build --workspace --release

clean: ## Remove build output
	cargo clean

fmt: ## Format the Rust code
	cargo fmt --all

fmt-check: ## Fail if the Rust code is not formatted
	cargo fmt --all -- --check

lint: ## Clippy with warnings denied
	cargo clippy --workspace --all-targets -- -D warnings

test: ## Run every Rust test
	cargo test --workspace

doc: ## Build the API docs with warnings denied
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

ci: fmt-check lint test doc ## Everything CI runs

bench: ## Run the Criterion benches
	cargo bench --workspace

install: ## Install the pptxboss CLI from this checkout
	cargo install --path crates/pptxboss-cli --locked --force

##@ Python

develop: ## Build the extension into the active virtualenv
	maturin develop

test-py: develop ## Run the Python tests against a fresh develop build
	pytest -q

wheel: ## Build a release wheel into dist/
	maturin build --release

##@ Docs

book: ## Build the mdBook into docs/book
	mdbook build docs

book-serve: ## Serve the mdBook with live reload
	mdbook serve docs

##@ Help

help: ## Show this help
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) }' $(MAKEFILE_LIST)

.PHONY: build release clean fmt fmt-check lint test doc ci bench install develop test-py wheel book book-serve help
