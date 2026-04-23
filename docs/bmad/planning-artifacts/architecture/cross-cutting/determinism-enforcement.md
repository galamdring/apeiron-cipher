# Cross-Cutting Concern: Determinism Enforcement

Seed-based generation must produce identical results given identical inputs. This constrains: RNG (seeded only), floating-point operations (platform-consistent), and any async operations that could introduce non-determinism. **System ordering requires an explicit strategy** — a documented schedule graph defining execution order for all gameplay systems, not just a general commitment. In Bevy, system ordering is the single most common source of subtle determinism bugs.
