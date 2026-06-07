# Cross-Cutting Concern: Mirror System

Observes player behavior across all gameplay systems and deepens the world in the direction of player interest. Every system that produces observable player actions must implement observation hooks. Not a standalone plugin — it is a cross-cutting contract.

**Observable behaviors and Mirror responses (additions with GDD v1.1):**

- **Giant flora inhabitation:** time spent in flora interiors, hazard interaction outcomes, and the choice to establish a base inside flora are all observable. If the player consistently inhabits flora, Mirror deepens flora presence and increases seasonal complexity in the world around them. Mirror hooks must be present from initial flora interior implementation.
- **Ship repair engagement:** which components the player repairs first, which materials they prioritize, and total time spent on the ship are observable. Mirror hooks must be present from initial ship repair implementation — not deferred to a later pass. Observation during the repair arc informs how Mirror shapes subsequent world generation (e.g. density of compatible materials in new areas).
- **Terrain material investigation:** if the player examines terrain texture as a form of material knowledge — identifying surface composition through visual inspection — Mirror increases the density of those material types in subsequently generated terrain. The observation hook must fire on material identification events, not just on pickup or fabrication.
