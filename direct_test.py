#!/usr/bin/env python3
"""Direct openai client test to confirm Claude works with Copilot headers."""
import os
import sys

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

from openai import OpenAI

client = OpenAI(
    api_key=token,
    base_url="https://api.githubcopilot.com",
    default_headers={
        "Editor-Version": "vscode/1.104.1",
        "User-Agent": "HermesAgent/1.0",
        "Copilot-Integration-Id": "vscode-chat",
        "Openai-Intent": "conversation-edits",
        "x-initiator": "agent",
    }
)

print("Testing claude-sonnet-4.6...")
resp = client.chat.completions.create(
    model="claude-sonnet-4.6",
    messages=[{"role": "user", "content": "Say exactly 'Claude via Copilot working' and nothing else."}],
    max_tokens=20,
)
print("Response:", resp.choices[0].message.content)
