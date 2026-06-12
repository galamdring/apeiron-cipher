#!/usr/bin/env python3
"""
Exchange GitHub token for Copilot API token, then test Claude via openai client.
This replicates what Hermes does internally.
"""
import json
import os
import sys
import urllib.request

# Load token from .env
token = ""
env_file = "/Users/lmckechn/.hermes/profiles/developer/.env"
prefix = "GITHUB_TOKEN"
for line in open(env_file):
    line = line.strip()
    if line.startswith(prefix + "="):
        token = line[len(prefix) + 1:]
        break

if not token:
    print("No token found")
    sys.exit(1)

# Step 1: Exchange for short-lived Copilot API token
exchange_url = "https://api.github.com/copilot_internal/v2/token"
req = urllib.request.Request(
    exchange_url,
    method="GET",
    headers={
        "Authorization": f"token {token}",
        "User-Agent": "GitHubCopilotChat/0.26.7",
        "Accept": "application/json",
        "Editor-Version": "vscode/1.104.1",
    },
)

print("Exchanging token...")
with urllib.request.urlopen(req, timeout=10) as resp:
    data = json.loads(resp.read().decode())

copilot_token = data.get("token", "")
expires_at = data.get("expires_at", 0)
print(f"Got Copilot token (expires: {expires_at}), prefix: {copilot_token[:15]}...")

# Step 2: Use the exchanged token with Claude
from openai import OpenAI

client = OpenAI(
    api_key=copilot_token,
    base_url="https://api.githubcopilot.com",
    default_headers={
        "Editor-Version": "vscode/1.104.1",
        "User-Agent": "HermesAgent/1.0",
        "Copilot-Integration-Id": "vscode-chat",
        "Openai-Intent": "conversation-edits",
        "x-initiator": "agent",
    }
)

print("\nTesting claude-sonnet-4.6 with exchanged token...")
resp = client.chat.completions.create(
    model="claude-sonnet-4.6",
    messages=[{"role": "user", "content": "Say exactly 'Claude via Copilot working' and nothing else."}],
    max_tokens=20,
)
print("Response:", resp.choices[0].message.content)
