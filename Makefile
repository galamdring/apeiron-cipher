.PHONY: build test lint fmt-check fmt run check clean install-hooks

build:
	cargo build

test:
	cargo test

lint:
	cargo clippy -- -D warnings

fmt-check:
	cargo fmt --check

fmt:
	cargo fmt

run:
	cargo run

check: fmt-check lint test build

clean:
	cargo clean

install-hooks:
	cp hooks/pre-commit .git/hooks/pre-commit
	chmod +x .git/hooks/pre-commit
	@echo "Pre-commit hook installed."
