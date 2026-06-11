# ADR 001 — CrewAI Flow Over LangChain / Custom Dispatch

**Status:** Accepted
**Date:** 2026-06

---

## Context

The Apeiron Cipher game repo uses GitHub Issues as its work queue. We need a
system that can:

1. Pick up an issue, understand its requirements, write Rust/Bevy code that
   satisfies them, and open a PR — autonomously.
2. Review the resulting PR against the game's architecture rules and post a
   structured GitHub review.
3. Respond to @mentions on issues and PRs in a multi-turn conversational loop.

The original approach was a single Python script (`poc.py`) calling the GitHub
API directly and driving an LLM via the OpenAI API. It worked for one-shot
tasks but had no state persistence, no retry logic, no structured crew handoffs,
and no separation between the "write code" and "review code" concerns.

---

## Decision

Use **CrewAI Flow** as the orchestration layer.

---

## Why CrewAI Flow

**Structured multi-crew pipelines.** The dev→review→respond separation maps
naturally to CrewAI's Flow + Crew model. Each concern gets its own crew with
its own agents, tools, and output schema. The Flow wires them together with
typed state and explicit routing.

**Built-in persistence.** `@persist` + `SQLiteFlowPersistence` gives us
resume-by-default for free. Issue worktrees and flow state survive process
restarts without building a custom checkpoint system.

**Pydantic output schemas.** `ReviewVerdict` and `RespondResult` are typed
Pydantic models. The crew is required to produce structured output. This
eliminates fragile text parsing to determine what the crew decided.

**LiteLLM backend.** CrewAI uses LiteLLM, which gives us a single model string
(`github_copilot/claude-sonnet-4.6`) that works across providers without
changing any crew code. Switching models is a one-line env var change.

**Conversational flows.** `ConversationState` + `RouterConfig` give RespondFlow
a multi-turn memory model without building a custom session store. Each mention
is a turn; the flow routes based on classification.

---

## Why Not LangChain

LangChain's agent abstractions add indirection without adding value for this use
case. We need explicit control over: which tools each crew has, what structured
output each crew produces, and how failures propagate. LangChain's default agent
loop obscures all of that. CrewAI's explicit crew/agent/task model matches our
mental model more directly.

---

## Why Not a Custom Dispatch System

The Go orchestrator in the game repo handles Hermes Kanban dispatch. That system
works well for human-facing task management. For autonomous code implementation,
we need: worktree lifecycle management, GitHub App authentication, structured LLM
output, and multi-turn conversation state. Building all of that custom would
reproduce what CrewAI already provides.

---

## Consequences

**Good:**
- Each crew (dev, review, respond) is independently testable
- Flow state is serializable — useful for debugging failed runs
- Model changes don't require code changes
- Adding a new crew (e.g. triage) is additive, not a rewrite

**Accepted costs:**
- CrewAI is a dependency with its own release cadence — updates may require
  code changes
- `crewai.experimental.conversational` is explicitly experimental — if it
  changes or is removed, RespondFlow needs a new session management approach
- The `@persist` decorator and `SQLiteFlowPersistence` are CrewAI internals —
  we are coupled to their schema for state persistence

---

## Alternatives Considered

| Option | Why Rejected |
|---|---|
| LangChain agents | Too much implicit behavior; unclear failure modes |
| Raw OpenAI API + custom loop | Reinventing persistence, routing, structured output |
| Hermes Kanban workers | Good for human-facing tasks; not designed for autonomous code loops |
| AutoGen | Multi-agent chat model doesn't fit the structured pipeline shape |
