# Cross-Cutting Concern: Asset Pipeline Architecture

All game tuning and material properties reside in data files, making the asset pipeline the nervous system of the application. Must support: hot-reloading for dev iteration, versioned schemas for save/load compatibility across game versions, and async loading that does not break determinism guarantees. This is infrastructure that every plugin depends on.

**Terrain texture derivation:**
Terrain texture is generated at runtime from material property data. The pipeline supports this derivation path alongside hot-reloading for source material parameter files. There are no pre-authored terrain texture assets — the texture is a runtime output of the material property vector.

**Flora collision geometry validation:**
Flora mesh assets must have exact collision geometry derived from the visual surface mesh. No flora asset is accepted with bounding box or convex hull collision. The pipeline validates the presence of a surface-traced collision mesh on load.
