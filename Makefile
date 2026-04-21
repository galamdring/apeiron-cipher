.PHONY: build test lint fmt-check fmt run check clean install-hooks kb-check o-check coverage coverage-html

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

# --- Kanban checks ---
kb-check:
	cd kanban && npm run lint && npm test && npm run build

# --- Orchestrator checks ---
o-check:
	cd infra/orchestrator && go vet ./... && go test ./...

install-hooks:
	cp hooks/pre-commit .git/hooks/pre-commit
	chmod +x .git/hooks/pre-commit
	@echo "Pre-commit hook installed."

coverage:
	cargo llvm-cov --summary-only

coverage-html:
	cargo llvm-cov --html --open
