# crewai-orchestrator/Makefile — Python orchestrator developer targets
#
# o-check: ruff lint+format + pytest on new triage modules
#          Run this before committing Python orchestrator changes.
#
# Called from the root Makefile's o-check target.

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

.PHONY: o-check lint typecheck test install

# Default target
o-check: lint test

install:
	cd apeiron_flow && uv sync --dev

lint:
	@echo "==> ruff check (triage modules + tests)"
	$(RUFF) check \
	  apeiron_flow/src/apeiron_flow/config.py \
	  apeiron_flow/src/apeiron_flow/labels.py \
	  apeiron_flow/src/apeiron_flow/main.py \
	  apeiron_flow/src/apeiron_flow/tools/triage_tools.py \
	  apeiron_flow/src/apeiron_flow/crews/triage_crew/ \
	  apeiron_flow/tests/ \
	  --select E,F,W,I --ignore E501
	@echo "==> ruff format check"
	$(RUFF) format --check \
	  apeiron_flow/src/apeiron_flow/config.py \
	  apeiron_flow/src/apeiron_flow/labels.py \
	  apeiron_flow/src/apeiron_flow/main.py \
	  apeiron_flow/src/apeiron_flow/tools/triage_tools.py \
	  apeiron_flow/src/apeiron_flow/crews/triage_crew/ \
	  apeiron_flow/tests/

test:
	@echo "==> pytest"
	PYTHONPATH=$(SRC) $(PYTEST) $(TESTS) -v --tb=short
