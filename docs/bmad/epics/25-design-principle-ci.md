# Epic 25: Design-Principle CI Gates

**GitHub Issue:** TBD (file from this spec; GitHub remains authoritative once numbered)
**Ring:** Cross-cutting — Process / Guard Rails
**Dependencies:** None. Can land in parallel with gameplay. Does not replace Phase 1 of the course correction; it makes later drift fail the build.
**Origin:** Course-correction review — `docs/bmad/design-drift-and-remediation.md` (PR #509) and GDD pillars (Learning First, diegetic-only, accretion).

---

## Goal

Make the GDD guard rails mechanically unskippable. A story that confirms, lists `???` slots, stamps `"Confirmed"`, or cheats diegetic UI with a HUD overlay must fail CI — not wait for a human to reread the brief.

This epic does **not** fix the existing violations. It installs a **ratchet**: current matches live in a baseline file; **new** matches fail; **removed** matches must leave the baseline. Gameplay stories (cold-start restage, stop confirming) shrink the baseline to empty.

## Why CI, not only review

Agents close well-specified stories. The GDD is not currently an acceptance criterion on the issue template. Review checklists get skipped. The lint-suppression guard already proved this pattern works in this repo: a script plus CI, not a reminder.

Some design tests cannot be fully automated (accretion, mirror, 30-minute boot). Those stay as required PR checkboxes. Everything that *can* be grepped or booted in a headless App becomes a gate.

## Design tests (from the course correction)

| # | Test | CI-enforceable now? | Story |
|---|---|---|---|
| 1 | **Accretion** — player understands something new without a panel | Partial (no instructional HUD chrome) | 25.2, 25.7 |
| 2 | **Confirmation** — UI must not say the player is correct, legal, or finished | Yes | 25.2 |
| 3 | **Skip-learning** — no names, recipe strings, green-legal crosshairs, `???` slots | Mostly | 25.2, 25.4 |
| 4 | **Verb** — change strengthens explore / navigate / interact / try / talk | No (human) | 25.7 |
| 5 | **Mirror** — world responds to interest, not only a seed | Later | 25.5 |
| 6 | **Playable in 30 minutes of boot** | No (human playtest) | 25.7 |

## Deliverable

- `make design-check` in `make check`
- CI job on every PR
- Pre-commit when `src/` Rust changes
- Rule catalog + baseline ratchet
- PR template with the six tests

## Stories

### 25.1: Checker harness and baseline ratchet

**This story.** Script, rules file, baseline, Makefile, GitHub Actions, pre-commit, self-test.

**Acceptance:**

- `python3 scripts/check-design-principles.py` exits 0 on current `main` (violations listed in baseline).
- Adding a new player-facing `"Confirmed"` (or other gated pattern) outside the baseline fails the script.
- Removing a violation without deleting its baseline row fails (stale baseline).
- `make check` runs the script.
- A line may carry `// design-principle-allow: RULE_ID — reason` to suppress a match. Suppressions require a reason.

### 25.2: Confirmation-language rules

Gate player-facing confirmation and fog-of-war stat sheets.

| ID | Pattern | Principle |
|---|---|---|
| `DP-CONFIRM-CONFIRMED` | `"Confirmed"` / `[Confirmed]` as a confidence stamp | Never confirm |
| `DP-CONFIRM-UNKNOWN-SLOT` | `???` property slots | Omit unknown; do not list empty stats |
| `DP-CONFIRM-OBS-COUNT` | `({n} obs)` in journal list copy | No numbers in player-facing UI |

Landed with 25.1 as the first rule set (baseline-recorded). Later gameplay PRs delete the rows.

### 25.3: Diegetic HUD gate

- Screen-space `Node` + `Text` that is not the F3 debug overlay.
- `DiegeticSurfaceKind::Readable { perceivable_range: 0.0, ... }` (the journal cheat).
- Headless App test: after `add_game_plugins`, every `Text` is either debug-marked or a real in-world surface.

### 25.4: Skip-learning identity gate

- Spawn-time well-known display names in player-facing UI (`WellKnownMaterial::display_name` piped to journal/examine).
- `compositional_name` used as the fabricated material's shown name.
- Crosshair color as legality legend (green = can pick up).

### 25.5: Knowledge-as-power gate

At least one **simulation** system (not journal, not examine panel) queries `KnowledgeGraph` to change world behavior or appearance. A CI test that only the presentation layer reads the graph fails until a gameplay system does.

### 25.6: Intent / diegetic-response harness

Authority ADR: every `Intent` rejection emits a `DiegeticResponse`. Marker traits + integration-test harness. Larger; depends on introducing the traits without a networking rewrite.

### 25.7: PR and story definition-of-done

Pull-request template lists the six design tests. Story issues must include them. Human tests (4, 6) stay checkboxes; CI covers 2 and 3.

## Out of scope

- Fixing journal, examine, naming, or the workshop cold start (gameplay Phase 1).
- Kanban, BMAD, or orchestrator CI.
- Replacing `cargo test` or clippy.

## Ratchet contract

```
new match not in baseline  → fail (drift)
match in baseline          → warn, do not fail
baseline row with no match → fail (update the baseline; the list only shrinks)
```

`--update-baseline` is for humans shrinking the list after a real fix, not for adding new rows in feature PRs. Feature PRs that would need a new baseline row are failing the principle and should change the feature, not the baseline.

## References

- Game Brief pillars; GDD Learning First, Accretion Test, First Fifteen Minutes
- `docs/bmad/project-context.md` §Game Design Guard Rails
- `docs/bmad/planning-artifacts/architecture/cross-cutting/diegetic-feedback.md`
- `docs/bmad/planning-artifacts/architecture/decisions/authority-boundary-pattern.md`
- Lint-suppression guard: `scripts/check-lint-suppression.sh`
