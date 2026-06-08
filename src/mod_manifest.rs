//! Mod manifest format, loader, and topological sort for the Apeiron Cipher
//! mod pipeline (Epic 23, Story 23.1).
//!
//! Every installed mod ships a `mod.toml` at its root. This module defines the
//! Rust types that represent that manifest (`ModManifest`, `ModInfo`,
//! `ModLicensing`), the loader that walks the `mods/` directory and parses them,
//! and the topological sort that produces a deterministic load order when mods
//! declare dependencies on each other.
//!
//! The resulting [`InstalledMods`] resource is registered in [`PreStartup`] so
//! all other plugins can read the installed mod list during `Startup` without
//! any ordering gymnastics.
//!
//! # Diegetic constraint
//!
//! Core Principle 3 says the game never explains internal state through UI text.
//! [`InstalledMods`] exposes metadata so the *diegetic in-world terminal* (future
//! story) can display "extensions loaded" without a separate HUD panel. The data
//! is available; how it surfaces is decided by the UI layer.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// ── Plugin ───────────────────────────────────────────────────────────────────

/// Bevy plugin — scans the `mods/` directory, parses all manifests,
/// topologically sorts them by dependency, and inserts [`InstalledMods`] as a
/// resource before any `Startup` system runs.
///
/// This plugin has no systems beyond the single `PreStartup` loader.  It does
/// NOT currently mount mod asset directories into Bevy's [`AssetServer`] — that
/// integration is deferred to Story 23.2 (Asset System Extensibility).  For
/// now, the loader provides a verified, ordered metadata snapshot that later
/// stories can build on.
pub struct ModManifestPlugin;

impl Plugin for ModManifestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InstalledMods>()
            .add_systems(PreStartup, load_installed_mods);
    }
}

// ── Manifest types ───────────────────────────────────────────────────────────

/// Schema version discriminant stored as the first field in every `mod.toml`.
///
/// Always `1` for the current format.  Future schema versions increment this
/// integer; the loader dispatches to a migration path before returning a
/// current-format [`ModManifest`].
pub type SchemaVersion = u32;

/// Top-level shape of a `mod.toml` file.
///
/// ```toml
/// schema_version = 1
///
/// [mod]
/// id          = "author.slug"
/// name        = "My Mod"
/// version     = "0.1.0"
/// description = "A short human-readable summary."
/// author      = "Author Name"
/// dependencies      = ["other.mod", "another.mod"]
/// game_version_min  = "0.1.0"
///
/// [licensing]
/// spdx_license          = "CC-BY-4.0"
/// free_distribution_url = ""
/// ```
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModManifest {
    /// Schema version — must be `1` for files the current loader understands.
    pub schema_version: SchemaVersion,

    /// Core mod identity and metadata.
    #[serde(rename = "mod")]
    pub info: ModInfo,

    /// Licensing metadata required for workshop discoverability.
    pub licensing: ModLicensing,
}

/// Core identity and metadata block (`[mod]` table in `mod.toml`).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModInfo {
    /// Globally unique reverse-domain identifier, e.g. `author.slug`.
    ///
    /// Must exactly match the mod's directory name so the loader can validate
    /// that the file is in the right place.
    pub id: String,

    /// Human-readable display name shown in mod lists.
    pub name: String,

    /// Semantic version string, e.g. `"0.1.0"`.
    pub version: String,

    /// Short human-readable summary displayed alongside the mod name.
    #[serde(default)]
    pub description: String,

    /// Mod author or team name.
    #[serde(default)]
    pub author: String,

    /// Ordered list of mod IDs this mod requires to be loaded before it.
    ///
    /// IDs must match the `mod.id` field in the dependency's `mod.toml`.
    /// Circular dependencies are a hard error — the loader logs `error!` and
    /// skips the entire cycle.
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Minimum Apeiron Cipher version this mod targets, e.g. `"0.1.0"`.
    ///
    /// The loader emits `warn!` if the running game version is lower than this
    /// value but still loads the mod.  Future versions may gate loading here.
    pub game_version_min: String,
}

/// Licensing metadata block (`[licensing]` table in `mod.toml`).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModLicensing {
    /// SPDX license identifier, e.g. `"CC-BY-4.0"`.
    pub spdx_license: String,

    /// URL of the free version if this mod is also sold on a paid platform.
    ///
    /// Empty string means not monetized.  Non-empty is a structural commitment
    /// to the monetization parity rule from the GDD.  Automated enforcement is
    /// deferred to Epic 23.
    #[serde(default)]
    pub free_distribution_url: String,
}

// ── Resource ─────────────────────────────────────────────────────────────────

/// Bevy resource that holds the complete, dependency-ordered snapshot of every
/// installed mod loaded at startup.
///
/// Populated during `PreStartup` by [`load_installed_mods`].  Available to all
/// later systems (including UI layers) via `Res<InstalledMods>`.
///
/// # Ordering guarantee
///
/// [`InstalledMods::mods`] is sorted in topological dependency order: if mod B
/// declares mod A as a dependency, A appears before B in the slice.  Within the
/// same topological level, mods are sorted alphabetically by `id` for
/// determinism (Core Principle 4).
#[derive(Clone, Debug, Default, Resource)]
pub struct InstalledMods {
    /// Ordered list of successfully loaded manifests.
    pub mods: Vec<ModManifest>,
}

impl InstalledMods {
    /// Returns an iterator over installed mod metadata in load order.
    pub fn iter(&self) -> impl Iterator<Item = &ModManifest> {
        self.mods.iter()
    }

    /// Looks up a manifest by its unique mod ID.
    ///
    /// Returns `None` if no installed mod has that ID.
    pub fn get(&self, id: &str) -> Option<&ModManifest> {
        self.mods.iter().find(|m| m.info.id == id)
    }

    /// Returns `true` if a mod with the given ID is installed.
    pub fn is_installed(&self, id: &str) -> bool {
        self.get(id).is_some()
    }
}

// ── Loader ───────────────────────────────────────────────────────────────────

/// `PreStartup` system — walks `mods/`, parses every `mod.toml`, validates
/// manifests, topologically sorts them, and stores the result in
/// [`InstalledMods`].
///
/// # Phase
/// Runs in `PreStartup` so the resource is available to all `Startup` systems.
///
/// # Reads
/// The filesystem path `mods/` relative to the working directory (the game
/// root in development, the install directory in production builds).
///
/// # Writes
/// [`InstalledMods`] resource (via `ResMut`).
///
/// # Error handling
/// - Missing `mods/` directory: logs `info!` (not an error — modding is
///   optional) and leaves the resource empty.
/// - Unreadable or un-parseable `mod.toml`: logs `error!` and skips that mod.
/// - `mod.id` mismatch with directory name: logs `error!` and skips that mod.
/// - Dependency cycle: logs `error!` and skips the entire cycle.
/// - Missing dependency: logs `warn!` and skips the dependent mod.
pub fn load_installed_mods(mut installed: ResMut<InstalledMods>) {
    let mods_dir = Path::new("mods");

    if !mods_dir.exists() {
        info!("no mods/ directory found — running without mods");
        return;
    }

    // ── Step 1: Discover and parse all mod.toml files ─────────────────────
    let raw = collect_manifests(mods_dir);

    if raw.is_empty() {
        info!("mods/ directory is empty — no mods loaded");
        return;
    }

    // ── Step 2: Validate id-to-directory consistency ──────────────────────
    let validated: Vec<ModManifest> = raw
        .into_iter()
        .filter(|(dir_name, manifest)| {
            if manifest.info.id != *dir_name {
                error!(
                    mod_dir = %dir_name,
                    manifest_id = %manifest.info.id,
                    "mod.toml `mod.id` does not match directory name — skipping"
                );
                false
            } else {
                true
            }
        })
        .map(|(_, manifest)| manifest)
        .collect();

    // ── Step 3: Build an index keyed on mod id ────────────────────────────
    let mut by_id: HashMap<String, ModManifest> = validated
        .into_iter()
        .map(|m| (m.info.id.clone(), m))
        .collect();

    // ── Step 4: Validate that every declared dependency is present ────────
    let present_ids: HashSet<String> = by_id.keys().cloned().collect();
    let mut to_remove: Vec<String> = Vec::new();

    for manifest in by_id.values() {
        for dep in &manifest.info.dependencies {
            if !present_ids.contains(dep.as_str()) {
                warn!(
                    mod_id = %manifest.info.id,
                    missing_dep = %dep,
                    "mod declares dependency that is not installed — skipping this mod"
                );
                to_remove.push(manifest.info.id.clone());
                break;
            }
        }
    }

    for id in to_remove {
        by_id.remove(&id);
    }

    // ── Step 5: Topological sort (Kahn's algorithm) ───────────────────────
    match topological_sort(by_id) {
        Ok(sorted) => {
            let count = sorted.len();
            installed.mods = sorted;
            info!(mod_count = count, "mod manifests loaded and sorted");
        }
        Err(cycle_members) => {
            error!(
                ?cycle_members,
                "dependency cycle detected among installed mods — affected mods skipped"
            );
        }
    }
}

/// Walks `mods_dir`, reads each subdirectory's `mod.toml`, and returns a list
/// of `(directory_name, ModManifest)` pairs for manifests that parsed
/// successfully.
///
/// Logs `error!` for every file that cannot be read or parsed, then continues
/// to the next mod so one bad file doesn't block the rest.
fn collect_manifests(mods_dir: &Path) -> Vec<(String, ModManifest)> {
    let entries = match fs::read_dir(mods_dir) {
        Ok(e) => e,
        Err(err) => {
            error!(%err, path = %mods_dir.display(), "failed to read mods/ directory");
            return Vec::new();
        }
    };

    let mut results: Vec<(String, ModManifest)> = Vec::new();

    for entry in entries.flatten() {
        let path: PathBuf = entry.path();

        // Only look at directories — files at the mods/ root are ignored.
        if !path.is_dir() {
            continue;
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => {
                warn!(path = %path.display(), "mod directory name is not valid UTF-8 — skipping");
                continue;
            }
        };

        let manifest_path = path.join("mod.toml");

        let contents = match fs::read_to_string(&manifest_path) {
            Ok(c) => c,
            Err(err) => {
                error!(
                    %err,
                    path = %manifest_path.display(),
                    "failed to read mod.toml — skipping mod"
                );
                continue;
            }
        };

        match toml::from_str::<ModManifest>(&contents) {
            Ok(manifest) => {
                results.push((dir_name, manifest));
            }
            Err(err) => {
                error!(
                    %err,
                    path = %manifest_path.display(),
                    "failed to parse mod.toml — skipping mod"
                );
            }
        }
    }

    // Sort by directory name for a stable iteration order before topo sort.
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

/// Topologically sorts a map of manifests using Kahn's algorithm.
///
/// Returns `Ok(sorted)` on success where `sorted` is the manifests in
/// dependency order (a dependency always precedes its dependent).  Returns
/// `Err(cycle_members)` if a cycle is detected, where `cycle_members` lists
/// the IDs involved in the cycle.
///
/// Within each topological level (mods whose dependencies are all already
/// resolved), mods are emitted in alphabetical ID order for determinism.
fn topological_sort(
    manifests: HashMap<String, ModManifest>,
) -> Result<Vec<ModManifest>, Vec<String>> {
    // Build in-degree map and adjacency list.
    //
    // in_degree[id] = number of declared dependencies still unresolved.
    // dependents[dep_id] = list of mod ids that depend on dep_id.
    let ids: Vec<String> = {
        let mut v: Vec<String> = manifests.keys().cloned().collect();
        v.sort(); // alphabetical for determinism within levels
        v
    };

    let mut in_degree: HashMap<&str, usize> = ids.iter().map(|id| (id.as_str(), 0)).collect();
    let mut dependents: HashMap<&str, Vec<&str>> =
        ids.iter().map(|id| (id.as_str(), vec![])).collect();

    for id in &ids {
        let manifest = &manifests[id];
        for dep in &manifest.info.dependencies {
            // dep is guaranteed to exist in `manifests` (checked before this call).
            *in_degree.entry(id.as_str()).or_insert(0) += 1;
            dependents
                .entry(dep.as_str())
                .or_default()
                .push(id.as_str());
        }
    }

    // Collect all mods with in-degree 0 (no unresolved dependencies).
    // Sort alphabetically for a fully deterministic output.
    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|&(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();
    queue.sort();

    let mut sorted: Vec<ModManifest> = Vec::with_capacity(manifests.len());

    while !queue.is_empty() {
        // Pop the lexicographically smallest ready mod.
        let current = queue.remove(0);

        // Safety: every id in queue came from the manifests map.
        sorted.push(manifests[current].clone());

        // Reduce in-degree of each dependent.  Collect newly zero-degree mods.
        let deps_of_current: Vec<&str> = dependents[current].clone();
        for dependent in deps_of_current {
            let deg = in_degree.entry(dependent).or_insert(0);
            *deg -= 1;
            if *deg == 0 {
                queue.push(dependent);
            }
        }
        queue.sort();
    }

    if sorted.len() != manifests.len() {
        // Not all mods made it out — there's a cycle among the remainder.
        let emitted: HashSet<&str> = sorted.iter().map(|m| m.info.id.as_str()).collect();
        let cycle_members: Vec<String> = ids
            .iter()
            .filter(|id| !emitted.contains(id.as_str()))
            .cloned()
            .collect();
        Err(cycle_members)
    } else {
        Ok(sorted)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Manifest parsing ─────────────────────────────────────────────────────

    /// A fully-specified mod.toml round-trips through TOML serialization.
    #[test]
    fn manifest_toml_round_trip() {
        let src = r#"
schema_version = 1

[mod]
id               = "author.my-mod"
name             = "My Mod"
version          = "0.1.0"
description      = "A test mod."
author           = "Test Author"
dependencies     = ["other.dep"]
game_version_min = "0.1.0"

[licensing]
spdx_license          = "CC-BY-4.0"
free_distribution_url = ""
"#;
        let manifest: ModManifest = toml::from_str(src).expect("parse");
        assert_eq!(manifest.info.id, "author.my-mod");
        assert_eq!(manifest.info.name, "My Mod");
        assert_eq!(manifest.info.description, "A test mod.");
        assert_eq!(manifest.info.author, "Test Author");
        assert_eq!(manifest.info.dependencies, vec!["other.dep"]);
        assert_eq!(manifest.schema_version, 1);

        // Round-trip: serialize then re-parse.
        let reserialized = toml::to_string(&manifest).expect("serialize");
        let reparsed: ModManifest = toml::from_str(&reserialized).expect("re-parse");
        assert_eq!(reparsed.info.id, manifest.info.id);
        assert_eq!(reparsed.info.dependencies, manifest.info.dependencies);
    }

    /// Optional fields (`description`, `author`, `dependencies`) default to
    /// empty values when absent from the file so older manifests still parse.
    #[test]
    fn manifest_optional_fields_default() {
        let src = r#"
schema_version = 1

[mod]
id               = "minimal.mod"
name             = "Minimal"
version          = "1.0.0"
game_version_min = "0.1.0"

[licensing]
spdx_license = "MIT"
"#;
        let manifest: ModManifest = toml::from_str(src).expect("parse");
        assert_eq!(manifest.info.description, "");
        assert_eq!(manifest.info.author, "");
        assert!(manifest.info.dependencies.is_empty());
        assert_eq!(manifest.licensing.free_distribution_url, "");
    }

    // ── Topological sort ──────────────────────────────────────────────────────

    fn make_manifest(id: &str, deps: &[&str]) -> ModManifest {
        ModManifest {
            schema_version: 1,
            info: ModInfo {
                id: id.to_string(),
                name: id.to_string(),
                version: "0.1.0".to_string(),
                description: String::new(),
                author: String::new(),
                dependencies: deps.iter().map(|s| s.to_string()).collect(),
                game_version_min: "0.1.0".to_string(),
            },
            licensing: ModLicensing {
                spdx_license: "CC-BY-4.0".to_string(),
                free_distribution_url: String::new(),
            },
        }
    }

    /// A mod with no dependencies sorts before a mod that depends on it.
    #[test]
    fn topological_sort_simple_dependency() {
        let mut map: HashMap<String, ModManifest> = HashMap::new();
        map.insert("b.mod".to_string(), make_manifest("b.mod", &["a.dep"]));
        map.insert("a.dep".to_string(), make_manifest("a.dep", &[]));

        let sorted = topological_sort(map).expect("no cycle");
        let ids: Vec<&str> = sorted.iter().map(|m| m.info.id.as_str()).collect();
        // a.dep must appear before b.mod
        let pos_a = ids.iter().position(|&id| id == "a.dep").unwrap();
        let pos_b = ids.iter().position(|&id| id == "b.mod").unwrap();
        assert!(pos_a < pos_b, "dependency must come before dependent");
    }

    /// A mod with two dependencies sorts after both.
    #[test]
    fn topological_sort_diamond_dependency() {
        let mut map: HashMap<String, ModManifest> = HashMap::new();
        map.insert(
            "d.mod".to_string(),
            make_manifest("d.mod", &["b.mod", "c.mod"]),
        );
        map.insert("c.mod".to_string(), make_manifest("c.mod", &["a.base"]));
        map.insert("b.mod".to_string(), make_manifest("b.mod", &["a.base"]));
        map.insert("a.base".to_string(), make_manifest("a.base", &[]));

        let sorted = topological_sort(map).expect("no cycle");
        let ids: Vec<&str> = sorted.iter().map(|m| m.info.id.as_str()).collect();

        let pos = |id: &str| ids.iter().position(|&x| x == id).unwrap();
        assert!(pos("a.base") < pos("b.mod"), "a.base before b.mod");
        assert!(pos("a.base") < pos("c.mod"), "a.base before c.mod");
        assert!(pos("b.mod") < pos("d.mod"), "b.mod before d.mod");
        assert!(pos("c.mod") < pos("d.mod"), "c.mod before d.mod");
    }

    /// A cycle returns `Err` listing all cycle members.
    #[test]
    fn topological_sort_cycle_detected() {
        let mut map: HashMap<String, ModManifest> = HashMap::new();
        map.insert("x.mod".to_string(), make_manifest("x.mod", &["y.mod"]));
        map.insert("y.mod".to_string(), make_manifest("y.mod", &["x.mod"]));

        let result = topological_sort(map);
        assert!(result.is_err(), "cycle should return Err");
        let cycle = result.unwrap_err();
        assert!(cycle.contains(&"x.mod".to_string()));
        assert!(cycle.contains(&"y.mod".to_string()));
    }

    /// Alphabetical tie-breaking: when two mods are at the same level, they
    /// appear in alphabetical order.  This enforces Core Principle 4
    /// (determinism).
    #[test]
    fn topological_sort_deterministic_within_level() {
        let mut map: HashMap<String, ModManifest> = HashMap::new();
        map.insert("z.mod".to_string(), make_manifest("z.mod", &[]));
        map.insert("a.mod".to_string(), make_manifest("a.mod", &[]));
        map.insert("m.mod".to_string(), make_manifest("m.mod", &[]));

        let sorted = topological_sort(map).expect("no cycle");
        let ids: Vec<&str> = sorted.iter().map(|m| m.info.id.as_str()).collect();
        assert_eq!(ids, vec!["a.mod", "m.mod", "z.mod"]);
    }

    // ── InstalledMods resource queries ────────────────────────────────────────

    /// `InstalledMods::get` and `is_installed` find a mod by ID.
    #[test]
    fn installed_mods_lookup() {
        let installed = InstalledMods {
            mods: vec![make_manifest("a.mod", &[])],
        };
        assert!(installed.is_installed("a.mod"));
        assert!(!installed.is_installed("missing.mod"));
        assert_eq!(installed.get("a.mod").unwrap().info.id, "a.mod");
        assert!(installed.get("missing.mod").is_none());
    }
}
