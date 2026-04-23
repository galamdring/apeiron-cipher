# Cross-Cutting Concern: Persistence / Save Architecture

Knowledge accumulation is the player's only progression; save corruption or loss is catastrophic and unrecoverable. The save system must: serialize deterministically, support version migration as schemas evolve, handle the full knowledge spectrum (not just checkpoints), and anticipate multiplayer save authority (distributed consensus problem in Ring 5). Persistence is not a late-stage feature — it is core infrastructure from Ring 1.
