FROM rust:bookworm

# System dependencies + Bevy build deps (headless, no display server)
RUN apt-get update && apt-get install -y --no-install-recommends \
    make \
    git \
    curl \
    ca-certificates \
    python3 \
    python3-pip \
    python3-venv \
    nodejs \
    npm \
    pkg-config \
    libudev-dev \
    libasound2-dev \
    libwayland-dev \
    libxkbcommon-dev \
    && rm -rf /var/lib/apt/lists/*

# Rust components
RUN rustup component add clippy rustfmt

# GitHub CLI
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
    | dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
    > /etc/apt/sources.list.d/github-cli.list \
    && apt-get update && apt-get install -y gh \
    && rm -rf /var/lib/apt/lists/*

# Graphite CLI
RUN npm install -g @withgraphite/graphite-cli

# AI agent: Aider (via pip, in a venv to avoid externally-managed-environment errors)
RUN python3 -m venv /opt/aider-venv \
    && /opt/aider-venv/bin/pip install --no-cache-dir setuptools wheel \
    && /opt/aider-venv/bin/pip install --no-cache-dir aider-chat \
    && ln -s /opt/aider-venv/bin/aider /usr/local/bin/aider

# OpenCode install — TBD, depends on distribution method.
# For now, Aider is the primary agent.
# When OpenCode has a binary release, add:
# RUN curl -fsSL https://opencode.ai/install.sh | bash

# Git configuration for agent commits
RUN git config --global user.name "Apeiron Agent" \
    && git config --global user.email "agent@apeiron-cipher.dev" \
    && git config --global init.defaultBranch main

# Container entrypoint script
COPY agent-entrypoint.sh /agent-entrypoint.sh
RUN chmod +x /agent-entrypoint.sh

WORKDIR /workspace
ENTRYPOINT ["/agent-entrypoint.sh"]
