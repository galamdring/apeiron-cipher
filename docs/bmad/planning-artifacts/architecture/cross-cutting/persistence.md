# Cross-Cutting Concern: Persistence / Save Architecture

Knowledge accumulation is the player's only progression; save corruption or loss is catastrophic and unrecoverable. The save system must: serialize deterministically, support version migration as schemas evolve, handle the full knowledge spectrum (not just checkpoints), and anticipate multiplayer save authority (distributed consensus problem in Ring 5). Persistence is not a late-stage feature — it is core infrastructure from Ring 1.

**Required persistence scope (non-exhaustive, grows with each feature):**

- **Giant flora base state:** player-placed structures inside flora interiors, flora seasonal open/closed state, and interior environmental state (atmospheric composition, light levels, chemical concentrations) must persist. Seasonal state is the product of simulation time accumulation — it must NOT be regenerated from seed on load. Regenerating from seed would erase the history of what happened to any player base inside.
- **Found ship repair state:** per-component repair status, structural integrity, and flight capability must persist. This is the player's first fabrication history. Losing it on reload would destroy the meaning of the repair progression.
