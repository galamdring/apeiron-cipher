#!/bin/bash
# Run the apeiron_flow CLI with all credentials loaded from .env
set -e

VENV=/Users/lmckechn/projects/crewai-poc/.venv
FLOW_DIR=/Users/lmckechn/projects/crewai-poc/apeiron_flow

# Load project .env (GitHub App creds, bot handle, etc.)
set -o allexport
source "$FLOW_DIR/.env"
set +o allexport

# Load Hermes .env for GITHUB_TOKEN
source /Users/lmckechn/.hermes/profiles/developer/.env
export GITHUB_TOKEN

exec "$VENV/bin/apeiron_flow" "$@"
