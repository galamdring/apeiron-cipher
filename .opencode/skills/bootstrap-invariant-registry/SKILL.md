---
name: bootstrap-invariant-registry
description: Evaluate a repository's documentation and codebase to produce an initial invariant contract registry as a knowledge graph of system and user contracts, with documentation, implementation, test, asset, and context-scoped relationship evidence. Use when the user says "bootstrap invariant registry" or "generate invariant registry" or "build invariant registry" or "bootstrap invariant knowledge graph".
---

# Bootstrap Invariant Registry

You are producing an initial invariant contract registry for this repository.

The registry is a knowledge graph of **contracts**. A contract is a durable fact or guarantee that other work can depend on being true. Contracts are versioned so future tooling can detect when assumptions become stale, but this skill only performs the initial bootstrap.

This skill produces a first-pass inventory from architecture documentation when available, and from code/tests/assets when documentation is missing or incomplete.

---

## Core Model

The graph contains contract nodes only.

Do **not** create graph nodes for documentation sections, code symbols, tests, assets, systems, tasks, or external references. Those are first-class references attached to contracts.

There are only two contract types:

- `system` — a contract between internal systems, modules, services, data flows, assets, workflows, or automation.
- `user` — a contract between the system and an external user, consumer, operator, developer, player, API client, CLI user, desktop user, or other interaction audience.

Every contract may declare `audience`. For `user` contracts, audience identifies the external interaction surface. For `system` contracts, audience identifies the internal consumer, subsystem, workflow, or integration surface that relies on the contract.

Examples:

```yaml
type: system
audience:
  - asset-pipeline
  - material-loading
```

```yaml
type: user
audience:
  - player
  - desktop-user
```

```yaml
type: system
audience:
  - rest-api
  - cli-command-execution
```

---

## Output

Produce one file: `registry.yaml`.

Ask the user where to place it if no architecture documentation directory is obvious. Otherwise default to the architecture documentation tree's invariant directory. For Apeiron Cipher / OpenSky, default to:

```text
docs/bmad/planning-artifacts/architecture/invariants/registry.yaml
```

For generic repositories, if an architecture docs directory is obvious, place it under:

```text
<architecture-docs-dir>/invariants/registry.yaml
```

Do not produce a changelog. Bootstrap establishes the initial current state; it does not record a version transition.

---

## `registry.yaml` Schema

```yaml
schema_version: 1
generated_at: "<ISO8601>"
repo_commit: "<git sha or null if unavailable>"
source: bootstrap-invariant-registry

contracts:
  - id: <stable-kebab-case-id>
    type: system                    # system | user
    audience:
      - <consumer, subsystem, workflow, interaction surface, or user audience>
    version: v1
    summary: >
      One to three sentences stating the current contract precisely. Write in
      present tense. Include what is guaranteed, what depends on it when known,
      and what it excludes or constrains.

    doc_refs:
      - path: <relative path from repo root>
        section: <exact heading text, including # prefix when applicable>
        line_start: <line number of the heading>
        line_end: <last line of the section, inclusive>
        relationship: authoritative  # authoritative | discusses

    implements:
      - path: <relative path from repo root>
        symbol: <optional symbol, type, function, module, plugin, command, endpoint, or workflow>
        notes: <why this code/workflow implements or owns the contract>

    tests:
      - path: <relative path from repo root>
        symbol: <optional test, fixture, script, check target, or scenario name>
        notes: <what aspect of the contract this validates>

    assets:
      - path: <relative path from repo root>
        notes: <how this asset, schema, data file, or config participates in the contract>

    contexts:
      - id: <doc-derived-or-source-derived-context-slug>
        summary: >
          The context where this contract touches one or more other contracts.
          The context is discovered from the docs, code, tests, or assets; it is
          not selected from a fixed vocabulary.
        touches:
          - contract_id: <other-contract-id>
            how: >
              Why and how this contract touches that specific target contract in
              this specific context.
            evidence:
              doc_refs:
                - path: <relative path from repo root>
                  section: <exact heading text, including # prefix when applicable>
                  line_start: <line number of the heading>
                  line_end: <last line of the section, inclusive>
              code_refs:
                - path: <relative path from repo root>
                  symbol: <optional symbol, type, function, module, plugin, command, endpoint, or workflow>
              test_refs:
                - path: <relative path from repo root>
                  symbol: <optional test, fixture, script, check target, or scenario name>
              asset_refs:
                - path: <relative path from repo root>

    downstream_risks:
      - <what breaks, silently diverges, or becomes stale if this contract changes without updating dependents>

    confidence: 0.0-1.0
```

### Required Fields

Every contract must include:

- `id`
- `type`
- `audience`
- `version`
- `summary`
- `doc_refs` (may be an empty list when inferred without documentation)
- `implements` (may be empty)
- `tests` (may be empty)
- `assets` (may be empty)
- `contexts` (may be empty)
- `downstream_risks`
- `confidence`

Use empty lists rather than omitting fields.

### Contract IDs

Contract IDs are stable semantic handles. Use kebab-case. Prefer concept-stable IDs over location-derived IDs.

Good IDs:

```text
material-properties-are-asset-authored
deterministic-generation-from-seeds
no-ui-spoilers
cli-invalid-input-exits-nonzero
```

Avoid IDs that merely encode file location:

```text
core-principles-section-three
material-doc-heading-two
```

---

## What Qualifies as a Contract

Create a `system` contract for durable internal guarantees such as:

- a boundary between systems, including what crosses it and what does not
- a stable identifier, seed value, schema, data shape, API response, or command behavior consumed by another system
- a data ownership rule, such as code-authored vs. asset-authored vs. user-authored state
- a module, plugin, scheduling, lifecycle, persistence, or registration guarantee
- an ordering, determinism, authority, synchronization, validation, or serialization guarantee
- an automation, agent, or workflow rule that other tooling relies on

Create a `user` contract for durable external guarantees such as:

- player/user-facing behavior or presentation constraints
- CLI behavior, exit codes, output formats, or command interaction rules
- API consumer expectations
- desktop/web app interaction guarantees
- user-visible recovery, feedback, error, or state preservation behavior
- externally observable behavior that downstream work can rely on

Do not create contracts for:

- private implementation details with no external or cross-system dependents
- transient runtime state
- one-off tactical choices not intended to remain stable
- performance tuning values with no correctness or experience implications
- prose that only gives rationale and does not state or imply a durable guarantee

---

## Context-Scoped Touches

A contract's `contexts` field captures where that source contract touches other contracts.

A context entry answers:

1. **What is the context where this source contract touches another contract?**
2. **Why/how does it touch that specific target contract in this context?**
3. **What evidence shows the connection between the source and that specific target?**

Do not duplicate information that already belongs on the source or target contract. The source contract has its own docs/code/tests/assets. The target contract has its own docs/code/tests/assets. A context touch only records the connection between them in that context.

Context IDs are discovered, not predefined.

- When documentation exists, derive context IDs from the relevant documentation section or concept.
- When documentation is missing, derive context IDs from the source area that revealed the connection, such as a module, test scenario, command, endpoint, workflow, or asset pipeline.

Examples:

```yaml
contexts:
  - id: material-seed-model
    summary: >
      In the material seed model context, asset-authored material data touches
      deterministic generation because material identity must remain stable when
      derived from seeds.
    touches:
      - contract_id: deterministic-generation-from-seeds
        how: >
          Seed-authored material definitions rely on deterministic generation so
          the same seed produces stable material identity across runs.
        evidence:
          doc_refs:
            - path: docs/bmad/planning-artifacts/architecture/cross-cutting/material-seed-model.md
              section: "## Seeded Material Definitions"
              line_start: 20
              line_end: 44
          code_refs:
            - path: src/materials.rs
              symbol: MaterialDefinition
          test_refs:
            - path: src/materials_tests.rs
              symbol: seeded_material_generation_is_stable
          asset_refs: []
```

```yaml
contexts:
  - id: cli-argument-parsing
    summary: >
      In the CLI argument parsing context, invalid input behavior touches command
      execution because invalid invocations must not start partially configured
      execution runs.
    touches:
      - contract_id: command-execution-starts-after-valid-input
        how: >
          Argument parsing must reject invalid input before command execution can
          observe or act on partially parsed state.
        evidence:
          doc_refs: []
          code_refs:
            - path: crates/example-cli/src/main.rs
              symbol: parse_args
          test_refs:
            - path: crates/example-cli/tests/invalid_args.rs
              symbol: invalid_args_do_not_execute_command
          asset_refs: []
```

---

## Workflow

### Phase 1 — Repo Orientation

1. Read `AGENTS.md` if it exists. If not, read `CONTRIBUTING.md`, `README.md`, or equivalent project guidance.
2. Identify:
   - where architecture, design, ADR, requirements, workflow, or product documentation lives
   - the primary language/framework/runtime
   - likely code, test, asset, config, or workflow surfaces that encode contracts
3. Record:
   - current timestamp for `generated_at`
   - current git commit SHA if available
   - selected output path for `registry.yaml`

### Phase 2 — Documentation-First Extraction

If architecture or design documentation exists, read it first.

For each section, ask:

> If this changed, would code, tests, assets, docs, workflows, users, or downstream semantic assumptions need to change?

If yes, extract or update a contract.

For each documentation-backed contract:

- create a stable contract `id`
- classify it as `system` or `user`
- populate `audience`
- write a precise present-tense `summary`
- attach exact `doc_refs` with path, heading, line range, and `authoritative` or `discusses`
- identify `implements`, `tests`, and `assets` where possible
- identify context-scoped touches to other contracts
- record downstream risks and confidence

### Phase 3 — Code/Test/Asset Fallback Extraction

If architecture documentation is missing or incomplete, infer provisional contracts from code, tests, assets, schemas, config, scripts, and workflows.

Useful evidence includes:

- public APIs, interfaces, traits, commands, endpoints, or events
- module boundaries and plugin registration
- ECS systems, schedules, startup order, or lifecycle hooks
- validators, assertions, parsers, serializers, and loaders
- tests, fixtures, snapshots, scripts, CI targets, and check commands
- asset directories, schemas, authored data, config files, and migrations
- user-facing text, UI flows, CLI output, API responses, and error handling

When inferring from code/tests/assets:

- leave `doc_refs: []` if no documentation reference exists
- populate `implements`, `tests`, and `assets` with the evidence that supports the inference
- use lower confidence than documentation-backed contracts unless tests or public interfaces make the contract very clear
- do not pretend inferred contracts are authoritative; make the uncertainty visible through the summary, evidence, and confidence

The absence of `doc_refs` is enough to show that a documentation reference is missing. Do not add a separate documentation status field.

### Phase 4 — Context Extraction

For each contract, look for documented or inferred contexts where it touches other contracts.

For every touch:

1. Identify the target contract.
2. State how/why the source touches that target in this context.
3. Attach evidence specific to that source-target connection.

If the likely target contract does not exist yet, create it if it qualifies as a contract. Otherwise do not create a touch.

Avoid vague touches. If you cannot explain how/why the source touches the target in a specific context, omit the touch or lower confidence on the source contract.

### Phase 5 — Deduplication and Scoping

Review all extracted contracts together:

1. Merge duplicate contracts that describe the same guarantee.
2. Split contracts that contain multiple separable guarantees with different audiences, implementations, tests, assets, or context touches.
3. Prefer fewer, stronger contracts over many speculative micro-contracts.
4. Ensure every context touch points to an existing contract ID.
5. Sort contracts alphabetically by `id`.
6. Use empty lists for missing `doc_refs`, `implements`, `tests`, `assets`, or `contexts`.

### Phase 6 — Write Output and Report

1. Write `registry.yaml` to the selected output path.
2. Report:
   - total contracts extracted
   - count by `type`: `system` and `user`
   - audiences discovered
   - documentation files covered by `doc_refs`
   - contracts with empty `doc_refs`
   - contracts with no tests attached
   - contracts with no implementation or asset references attached
   - total context touches extracted
   - low-confidence contracts or uncertain context touches

---

## Notes

- Bootstrap produces a current-state registry only.
- All contracts start at `version: v1`.
- The registry does not decide whether missing documentation is acceptable. It simply exposes contracts with empty `doc_refs` when no documentation was found.
- The registry can later be used to draft missing architecture documentation, but this skill does not generate those docs.
- Prefer explicit uncertainty over false precision.
- Keep the graph contract-centered: contracts are nodes; docs, code, tests, and assets are references attached to contracts.
