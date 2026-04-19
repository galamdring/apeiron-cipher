# Testing Architecture

**Decision: Three-tier test organization with unified harness, hand-crafted golden files + double-entry determinism, and separated fuzz/bench suites**

- **Category:** Testing Architecture
- **Priority:** Important (shapes architecture)
- **Affects:** Every plugin, CI pipeline, determinism guarantees, developer/agent workflow

**Test file organization — two locations, clear split rule:**
- **Unit tests:** `#[cfg(test)] mod tests` inside each module file. Test internal logic, private functions, edge cases within a single module. These run fast and don't need an `App`.
- **Integration tests:** Top-level `tests/` directory. Test plugin behavior through the ECS — minimal `App` setup, scripted inputs, `app.update()` N ticks, assert world state + events + log output. Organized by plugin: `tests/material_plugin.rs`, `tests/knowledge_plugin.rs`, etc.
- **Rule of thumb:** If it needs an `App`, it's an integration test in `tests/`. If it's testing a pure function or internal data structure, it's a unit test in-module.

**Separate pure logic from ECS wiring:**
- Core game logic (material combination math, seed derivation, knowledge graph operations) should be pure functions testable with no Bevy dependency. ECS systems that call that logic are tested separately with minimal `App` integration tests. This separation makes unit tests fast and integration tests focused on ECS behavior, not business logic.

**Test naming convention:**
- No `test_` prefix — `#[test]` already marks it. Names follow `<thing>_<scenario>_<expected>` pattern. Examples: `combine_two_metals_produces_alloy()`, `pick_up_overweight_item_emits_resistance()`, `knowledge_graph_bfs_bounded_by_depth()`. Descriptive enough that failure messages identify the broken behavior without reading the test body.

**Seed-instance consistency tests:**
- When the architecture says knowledge is seed-level (Decision 1 — Material Seed Canonicality), tests must prove that learning from one entity updates the behavior of other same-seed entities in inspect, journal, and other player-facing systems. If two entities share a seed and a property is discovered on one, the second entity must reflect that knowledge immediately. This is an explicit test category, not an implied consequence.

**Unified test harness** — shared utilities in `tests/common/mod.rs`:
- **App builders provide infrastructure only, NOT the plugin under test.** Builders configure `SchedulingPlugin` + a `TestObservabilityPlugin` that replaces the production tracing stack with the `Vec<LogEvent>` capture layer. Each integration test file adds its own plugin: `app.add_plugins(MaterialPlugin)`. This enforces the per-plugin-boundary rule from Decision 5.
- **`TestObservabilityPlugin` replaces, never layers.** In tests, you don't want JSON file output or a console layer. The test harness provides a test-mode observability plugin that redirects all tracing to the capture buffer only. No production tracing infrastructure active during tests.
- **Event assertions:** `assert_event_emitted::<T>(&app)`, `assert_no_event::<T>(&app)`. Wraps the boilerplate of reading `Events<T>` from the world.
- **Fixture utilities:** Load helpers for reading golden files and input fixtures from `tests/fixtures/`.
- **Determinism helpers:** Run-compare utility that executes a closure twice with identical inputs and asserts output equality.
- **Diegetic compliance enforcement — automatic, not opt-in.** The test harness automatically validates diegetic compliance on every integration test run. After each test's tick execution, the harness iterates all `Intent`-marked events fired during the test and asserts a corresponding `DiegeticResponse`-marked event exists for each. A system that rejects an intent without producing a diegetic response fails the test automatically. No plugin opts into this — the harness enforces it globally.

**Test parallelism (documented, not configured):** Cargo runs integration test files as separate binaries in parallel. Tests within each binary run serially. For Bevy testing with `App` instances, serial-within-binary is correct — each test gets its own `App`, no shared global state. This is Cargo's default behavior and is the correct behavior for this project.

**SchedulingPlugin gets its own ordering tests:** The phase pipeline is the determinism backbone. `tests/scheduling_plugin.rs` contains integration tests that assert ordering correctness: systems registered in `Intent` run before systems in `Simulation`, `apply_deferred` fires between every phase boundary. These don't test game logic — they test that the schedule is wired correctly. If a future contributor accidentally misconfigures a system set, these tests catch it.

**Determinism testing — hand-crafted golden files + double-entry + cross-tick save/load:**

- **Golden files are hand-crafted, independently verified expected outputs.** They are NOT auto-generated from the current code. Someone must understand what the correct output is and write the fixture file. If the code changes output, the test fails, and a human verifies the new output is correct before manually updating the fixture. There is no `make update-golden` target, no `UPDATE_GOLDEN` env var, no auto-generation. Golden files exist precisely to catch "the code is deterministic but wrong" — auto-generating them defeats this purpose entirely.
- **Golden file float handling:** Golden file comparisons use epsilon tolerance for floating-point values, not exact string matching. Material properties derived from seeds may produce platform-dependent float representations. The comparison utility in the test harness handles this.
- **Double-entry (run-compare):** For transformation validation — run the same operation twice with identical seed + inputs, assert outputs match. This validates that the code path is internally consistent without needing a stored reference. Particularly important for material derivation, world generation, and knowledge graph operations where the golden file would be complex but the determinism invariant is simple: "do it twice, get the same thing." Catches non-determinism. Complementary to golden files: golden files catch "changed but deterministic," double-entry catches "non-deterministic."
- **Cross-tick save/load determinism tests:** Run N ticks, save state, reload from save, run N more ticks. Compare against running 2N ticks uninterrupted from the same starting state. Validates that the save/load cycle doesn't introduce drift. This tests the persistence boundary, the determinism guarantee, and knowledge graph serialization in one pattern.

**Test fixtures — single directory, organized by plugin:**
- All test data lives in `tests/fixtures/`, organized by plugin subdirectory: `tests/fixtures/material_plugin/`, `tests/fixtures/knowledge_plugin/`, etc.
- Input fixtures (known seeds, pre-built registries, event sequences) and expected output fixtures (golden files) coexist in the same directory structure. Both are fixtures — the distinction is how the test uses them, not where they live.

**Property-based testing — separate suite, intentional runs:**
- `proptest` as a dev-dependency. Standardized — no `quickcheck`. `proptest` provides composable strategies for generating complex structured inputs (seeds, event sequences, material property vectors) and better shrinking to find minimal failing cases. `quickcheck` is the older, simpler alternative — `proptest` is strictly more capable for this project's needs (seed-space exploration, complex structured inputs).
- **Not part of `make check`.** Fuzz tests explore the seed space and input space stochastically — they're for intentional exploration sessions, not CI gates. A separate `make fuzz` target runs them.
- **Primary candidates:** Material seed derivation (invariants hold across random seeds), knowledge graph operations (append-only growth invariant, no orphan edges), world generation (determinism invariant across random seeds), intent validation (no panic on arbitrary input combinations).
- **Invariant-style assertions only.** Fuzz tests don't assert specific outputs. They assert invariants: "for any seed, derived material density is within [0.0, max]", "for any sequence of DiscoveryEvents, the knowledge graph has no orphan edges."

**Benchmark testing — baseline from Ring 1, separate suite:**
- `criterion` as a dev-dependency. Benchmarks in `benches/` directory (standard Cargo convention).
- **Baseline benchmarks established early** for hot paths identified in prior decisions: knowledge graph BFS traversal, material registry lookup by ID, material similarity computation, seed derivation. These form the performance regression safety net.
- **Not part of `make check`.** A `make bench` target runs them. Benchmarks are reference measurements, not CI pass/fail gates (threshold-based CI benchmarks are fragile and noisy). Developers/agents run `make bench` before and after performance-sensitive changes and compare.
- **Core benchmark suite grows with the codebase.** Each new hot-path system adds its benchmark when implemented. The benchmark suite is a living performance profile, not a one-time measurement.

**Makefile integration:**

| Target | What it runs | When |
|--------|-------------|------|
| `make check` | fmt + clippy + unit tests + integration tests + `--no-default-features` + `--all-features` | Every commit, CI gate |
| `make fuzz` | Property-based tests via proptest | Intentional exploration sessions |
| `make bench` | Criterion benchmarks | Before/after performance-sensitive changes |

**Integration with prior decisions:**
- Decision 2 (Scheduling): Integration tests use the full phase pipeline via SchedulingPlugin. Tests call `app.update()` which runs the complete `FixedUpdate` phase sequence with `apply_deferred` between phases. SchedulingPlugin's own tests verify ordering correctness.
- Decision 3 (Observability): `TestObservabilityPlugin` replaces the production tracing stack with capture-only. Log assertions available in every integration test by default.
- Decision 4 (Authority Boundary): Diegetic compliance enforced automatically by the test harness on every integration test run. Per-intent positive + negative test paths enforced by CI.
- Decision 5 (Plugin Graph): Per-plugin-boundary testing. Each integration test file tests one plugin. Test App builders provide infrastructure only.

**Rationale:** The split between unit and integration test locations follows standard Rust conventions and keeps fast unit tests close to the code they test while integration tests get a shared harness with App builders, event assertions, and tracing capture. Hand-crafted golden files provide independent verification that auto-generated snapshots cannot — they catch "deterministic but wrong" bugs because the expected output was verified by a human, not derived from the code under test. Double-entry and cross-tick save/load tests complement golden files by catching non-determinism and persistence drift respectively. Diegetic compliance enforcement at the harness level makes Decision 4's architectural constraint automatically tested rather than relying on each plugin author to remember. Fuzz and benchmark suites are separated into intentional targets because their value comes from focused exploration, not routine execution.
