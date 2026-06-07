# Cross-Cutting Concern: Determinism Enforcement

Seed-based generation must produce identical results given identical inputs. This constrains: RNG (seeded only), floating-point operations (platform-consistent), and any async operations that could introduce non-determinism. **System ordering requires an explicit strategy** — a documented schedule graph defining execution order for all gameplay systems, not just a general commitment. In Bevy, system ordering is the single most common source of subtle determinism bugs.

**Deterministic pipeline requirements (additions with GDD v1.1):**

- **Terrain texture derivation:** terrain texture parameters are part of the deterministic pipeline. The same `MaterialSeed` must produce the same texture parameters on every platform and every run. Texture parameter derivation is not exempt from platform-consistent floating-point rules.
- **Flora mesh and collision geometry generation:** flora mesh and collision geometry generation is deterministic. The chain `FloraMeshSeed → mesh → collision` is a single deterministic pipeline. Platform-consistent floating-point rules apply to mesh vertex generation. Any platform-specific floating-point deviation in mesh generation produces divergent collision geometry, which is a physics authority violation.
