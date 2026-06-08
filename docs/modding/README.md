# Apeiron Cipher — Modding Guide

Welcome. This guide tells you everything you need to start building a mod for
Apeiron Cipher. Read it top-to-bottom once, then use it as a reference.

## What you can do right now

Apeiron Cipher is data-driven by design (Core Principle 6). Everything the game
reads from `assets/` is already moddable today: materials, biomes, combinations,
carry tuning, star types, orbital layout, surface deposits, scene configuration,
and more. A mod is just a parallel directory tree that mirrors `assets/`.

Full compiled-logic modding (Bevy plugins, WASM) is deferred to Epic 23. This
guide covers what is available and stable today.

## Guide index

| Document | What it covers |
|---|---|
| [Mod Structure](./mod-structure.md) | Directory layout, `mod.toml` manifest |
| [Data File Formats](./data-formats.md) | TOML schemas for every moddable domain |
| [Extensibility Points](./extensibility-points.md) | What each asset domain controls and how |
| [Best Practices](./best-practices.md) | Compatibility, versioning, testing |

## Quick start

1. Create `mods/your-name.your-mod/` in the game data directory.
2. Copy `mod.toml` from [Mod Structure](./mod-structure.md) and fill in your info.
3. Create an `assets/` subdirectory inside your mod directory that mirrors the
   base game layout — only include the files you want to add or change.
4. Run the game. Your files load alongside base game assets via the same
   `AssetServer` pipeline.

## Foundational rule

Mods live entirely in data files. You do not need to recompile the game, modify
Rust source, or distribute binaries. If your mod adds a `.toml` or `.ron` file
in the right directory, it works.
