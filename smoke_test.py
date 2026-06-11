#!/usr/bin/env python3
"""Smoke test: does CrewAI + LiteLLM talk to Copilot correctly?"""
import os
import sys

env_file = "/Users/lmckechn/.hermes/profiles/developer/.env"
token = ""
for line in open(env_file):
    line = line.strip()
    if line.startswith("GITHUB_TOKEN="):
        token = line.split("=", 1)[1]
        break

if not token:
    print("No GITHUB_TOKEN found in .env")
    sys.exit(1)

os.environ["OPENAI_API_KEY"] = token
os.environ["OPENAI_API_BASE"] = "https://api.githubcopilot.com"

from crewai.llm import LLM

COPILOT_HEADERS = {
    "Editor-Version": "vscode/1.104.1",
    "User-Agent": "HermesAgent/1.0",
    "Copilot-Integration-Id": "vscode-chat",
    "Openai-Intent": "conversation-edits",
    "x-initiator": "agent",
}

llm = LLM(
    model="openai/claude-sonnet-4.6",
    base_url="https://api.githubcopilot.com",
    api_key=token,
    extra_headers=COPILOT_HEADERS,
)

resp = llm.call([{"role": "user", "content": "Say 'CrewAI + Copilot working' and nothing else."}])
print("Response:", resp)
