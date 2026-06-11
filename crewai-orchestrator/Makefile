# crewai-orchestrator/Makefile — Python orchestrator developer targets
#
# check: run pytest
# fmt:   ruff format (auto-fix)
# lint:  ruff check
# o-check: combined lint + test gate (called from root Makefile's o-check target)

VENV      := apeiron_flow/.venv
PYTHON    := $(VENV)/bin/python3
PIP       := $(PYTHON) -m pip
RUFF      := $(VENV)/bin/ruff
PYTEST    := $(VENV)/bin/pytest

# If running directly from this directory, venv is at .venv
ifneq ($(wildcard .venv),)
VENV      := .venv
RUFF      := .venv/bin/ruff
PYTEST    := .venv/bin/pytest
endif

SRC       := apeiron_flow/src
TESTS     := apeiron_flow/tests

# Modules that are lint-clean. Extend this list as each module is brought up to standard.
LINT_TARGETS := \
  apeiron_flow/src/apeiron_flow/__init__.py \
  apeiron_flow/src/apeiron_flow/github_http.py \
  apeiron_flow/tests/

.PHONY: o-check check lint fmt install

# Default target — full gate
o-check: lint check

install:
	cd apeiron_flow && uv sync --dev

lint:
	@echo "==> ruff check"
	$(RUFF) check $(LINT_TARGETS) --select E,F,W,I --ignore E501
	@echo "==> ruff format check"
	$(RUFF) format --check $(LINT_TARGETS)

fmt:
	@echo "==> ruff format"
	$(RUFF) format $(LINT_TARGETS)

check:
	@echo "==> pytest"
	PYTHONPATH=$(SRC) $(PYTEST) $(TESTS) -v --tb=short; \
	  e=$$?; [ $$e -eq 0 ] || [ $$e -eq 5 ] && exit 0 || exit $$e
