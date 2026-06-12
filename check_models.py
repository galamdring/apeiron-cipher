#!/usr/bin/env python3
"""Check available Copilot models."""
import json, os, sys, urllib.request

env_file = "/Users/lmckechn/.hermes/profiles/developer/.env"
token = ""
for line in open(env_file):
    if line.startswith("GITHUB_TOKEN="):
        token = line.strip().split("=", 1)[1]
        break

if not token:
    print("No token found")
    sys.exit(1)

req = urllib.request.Request(
    "https://api.githubcopilot.com/models",
    headers={"Authorization": f"Bearer {token}"}
)
with urllib.request.urlopen(req) as r:
    data = json.loads(r.read())

print("Available models:")
for m in data.get("data", []):
    print(f"  {m['id']}")
