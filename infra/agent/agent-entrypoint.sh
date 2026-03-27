#!/bin/bash
set -euo pipefail

# Agent container entrypoint.
# Manages repo checkout, then executes the AI agent with the provided prompt.
#
# Usage: docker run ... apeiron-agent:latest [agent-command]
# The prompt is mounted at /prompt.md by the n8n workflow.

REPO_URL="https://github.com/galamdring/apeiron-cipher.git"
WORKSPACE="/workspace"

# --- Repo setup ---
if [ ! -d "$WORKSPACE/.git" ]; then
  echo "[agent] First run — cloning repo..."
  git clone "$REPO_URL" "$WORKSPACE"
else
  echo "[agent] Repo exists — updating..."
  cd "$WORKSPACE"
  git fetch --all
  git checkout main
  git pull origin main
fi

cd "$WORKSPACE"

# --- Configure gh CLI auth ---
if [ -n "${GH_TOKEN:-}" ]; then
  echo "[agent] Configuring gh CLI auth..."
  echo "$GH_TOKEN" | gh auth login --with-token 2>/dev/null || true
fi

# --- Configure Graphite auth ---
if [ -n "${GT_TOKEN:-}" ]; then
  echo "[agent] Configuring Graphite CLI auth..."
  gt auth --token "$GT_TOKEN" 2>/dev/null || true
fi

# --- Execute the agent command ---
if [ $# -gt 0 ]; then
  echo "[agent] Running: $@"
  exec "$@"
else
  echo "[agent] No command provided. Exiting."
  exit 1
fi
