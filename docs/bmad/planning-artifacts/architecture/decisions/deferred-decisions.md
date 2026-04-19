# Deferred Decisions

## Decision 8: Replication Framework (Deferred to Ring 5)

- **Category:** Networking
- **Deferred because:** Single-player authority model is fully functional through Ring 1-4. The Intent/Simulation trust boundary (Decision 4) is designed so that multiplayer is a transport change, not an architecture change. Choosing a replication framework now would be speculative — the Bevy networking ecosystem will look different by Ring 5.
- **When to decide:** Ring 5, Epic 22 (Multiplayer) planning.
- **Constraints already locked:** Intent events are serializable. Simulation is authoritative. Seeds are the only entropy source in generation. These won't change.

## Decision 9: Transport Layer (Deferred to Ring 5)

- **Category:** Networking
- **Deferred because:** Tightly coupled to the replication framework choice. No value in selecting a transport protocol without knowing the replication model. The Bevy networking ecosystem (lightyear, replicon, etc.) bundles transport with replication.
- **When to decide:** Ring 5, Epic 22, alongside Decision 8.
- **Constraints already locked:** Same as Decision 8.

## Decision 10: Modding API Surface (Deferred to Ring 5)

- **Category:** Extensibility
- **Deferred because:** The modding API surface depends on which systems stabilize through Rings 1-4. Exposing an API before the internal architecture settles creates a backwards-compatibility burden that constrains future evolution. Data-driven design (TOML asset files) already provides informal moddability for content without an API.
- **When to decide:** Ring 5, Epic 23 (Modding / Community Tools) planning.
- **Constraints already locked:** Data-driven asset pipeline (Decision 7) means content modding is possible without code changes. Plugin architecture (Decision 5) means the internal structure is modular. These are prerequisites, not decisions.
