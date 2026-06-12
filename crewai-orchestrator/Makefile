# crewai-orchestrator/Makefile — Python orchestrator developer targets
#
# check: run pytest
# fmt:   ruff format (auto-fix)
# lint:  ruff check
# o-check: combined lint + test gate (called from root Makefile's o-check target)
#
# Works from two invocation contexts:
#   From opensky/ (repo root): make -C crewai-orchestrator o-check
#   From crewai-orchestrator/: make o-check
# Detects which context by checking for ../apeiron_flow/.venv (crewai-orchestrator/ cwd)
# vs apeiron_flow/.venv (repo root cwd).

# Default: invoked from repo root (opensky/)
VENV      := apeiron_flow/.venv
SRC       := apeiron_flow/src
TESTS     := apeiron_flow/tests

# If running directly from crewai-orchestrator/, adjust paths to parent
ifneq ($(wildcard ../apeiron_flow/.venv),)
VENV      := ../apeiron_flow/.venv
SRC       := ../apeiron_flow/src
TESTS     := ../apeiron_flow/tests
endif

PYTHON    := $(VENV)/bin/python3
PIP       := $(PYTHON) -m pip
RUFF      := $(VENV)/bin/ruff
PYTEST    := $(VENV)/bin/pytest

.PHONY: o-check lint fmt check install

# Default target
o-check: lint check

install:
	cd $(dir $(VENV)) && uv sync --dev

lint:
	@echo "==> ruff check"
	$(RUFF) check \
	  $(SRC)/apeiron_flow/__init__.py \
	  $(SRC)/apeiron_flow/github_http.py \
	  $(SRC)/apeiron_flow/config.py \
	  $(SRC)/apeiron_flow/labels.py \
	  $(SRC)/apeiron_flow/main.py \
	  $(SRC)/apeiron_flow/tools/triage_tools.py \
	  $(SRC)/apeiron_flow/crews/triage_crew/ \
	  $(TESTS)/ \
	  --select E,F,W,I --ignore E501
	@echo "==> ruff format check"
	$(RUFF) format --check \
	  $(SRC)/apeiron_flow/__init__.py \
	  $(SRC)/apeiron_flow/github_http.py \
	  $(SRC)/apeiron_flow/config.py \
	  $(SRC)/apeiron_flow/labels.py \
	  $(SRC)/apeiron_flow/main.py \
	  $(SRC)/apeiron_flow/tools/triage_tools.py \
	  $(SRC)/apeiron_flow/crews/triage_crew/ \
	  $(TESTS)/

fmt:
	@echo "==> ruff format"
	$(RUFF) format \
	  $(SRC)/apeiron_flow/__init__.py \
	  $(SRC)/apeiron_flow/github_http.py \
	  $(SRC)/apeiron_flow/config.py \
	  $(SRC)/apeiron_flow/labels.py \
	  $(SRC)/apeiron_flow/main.py \
	  $(SRC)/apeiron_flow/tools/triage_tools.py \
	  $(SRC)/apeiron_flow/crews/triage_crew/ \
	  $(TESTS)/

check:
	@echo "==> pytest"
	PYTHONPATH=$(SRC) $(PYTEST) $(TESTS) -v --tb=short; \
	  e=$$?; [ $$e -eq 0 ] || [ $$e -eq 5 ] && exit 0 || exit $$e
