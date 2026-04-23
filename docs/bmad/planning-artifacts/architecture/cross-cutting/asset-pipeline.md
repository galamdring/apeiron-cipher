# Cross-Cutting Concern: Asset Pipeline Architecture

All game tuning and material properties reside in data files, making the asset pipeline the nervous system of the application. Must support: hot-reloading for dev iteration, versioned schemas for save/load compatibility across game versions, and async loading that does not break determinism guarantees. This is infrastructure that every plugin depends on.
