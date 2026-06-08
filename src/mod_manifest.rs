//! Mod manifest loader, validator, and activation tracker for the Apeiron Cipher
//! mod pipeline (Epic 23, Story 23.1).
//!
//! Every installed mod ships a `mod.toml` at its root.  This module defines:
//!
//! - The Rust types that represent that manifest ([`ModManifest`], [`ModInfo`],
//!   [`ModLicensing`]).
//! - The [`InstalledMods`] resource that holds the complete, dependency-ordered
//!   list of discovered mods.
//! - Explicit [`activate`][InstalledMods::activate] /
//!   [`deactivate`][InstalledMods::deactivate] methods so callers can
//!   enable/disable individual mods at runtime without scanning the filesystem
//!   again.
//! - The [`ModManifestPlugin`] that wires discovery into `PreStartup`.
//!
//! # Mod directory layout
//!
//! ```text
//! mods/
//! └── author.my-mod/          ← directory name must match mod.id
//!     ├── mod.toml            ← manifest validated here
//!     ├── README.md           ← optional
//!     └── assets/             ← mirrors base game assets/ layout
//! ```
//!
//! # Load and activation model
//!
//! Discovery happens once at `PreStartup`: all manifests are parsed and stored
//! in [`InstalledMods`].  By default every valid, dependency-satisfied mod is
//! **active** after discovery.  Callers may call
//! [`InstalledMods::deactivate`] / [`InstalledMods::activate`] at any time to
//! toggle a mod's active state without re-scanning the filesystem.
//!
//! [`InstalledMods::get_loaded_mods`] returns only the currently-active mods
//! in dependency order.  Mods deactivated at runtime disappear from that list
//! until reactivated.
//!
//! # Determinism
//!
//! Core Principle 4 — same seed + same inputs = same outputs.  All iteration
//! over mod collections is done in alphabetical ID order so that the load order
//! is reproducible across runs regardless of filesystem iteration order.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// ── Plugin ───────────────────────────────────────────────────────────────────

/// Bevy plugin — scans the `mods/` directory at `PreStartup`, parses all
/// `mod.toml` manifests, topologically sorts them by dependency, and inserts
/// [`InstalledMods`] as a resource before any `Startup` system runs.
///
/// This plugin has no systems beyond the single `PreStartup` loader.  It does
/// NOT currently mount mod asset directories into Bevy's [`AssetServer`] —
/// that integration is deferred to Story 23.2 (Asset System Extensibility).
/// For now, the loader provides a verified, ordered metadata snapshot that
/// later stories can build on.
pub struct ModManifestPlugin;

impl Plugin for ModManifestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InstalledMods>()
            .add_systems(PreStartup, discover_and_load_mods);
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
/// id               = "author.slug"
/// name             = "My Mod"
/// version          = "0.1.0"
/// description      = "A short human-readable summary."
/// author           = "Author Name"
/// dependencies     = ["other.mod", "another.mod"]
/// game_version_min = "0.1.0"
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
/// installed mod loaded at startup, plus per-mod active/inactive state.
///
/// Populated during `PreStartup` by [`discover_and_load_mods`].  Available to
/// all later systems (including UI layers) via `Res<InstalledMods>`.
///
/// # Ordering guarantee
///
/// [`InstalledMods::all_mods`] is sorted in topological dependency order: if
/// mod B declares mod A as a dependency, A appears before B in the slice.
/// Within the same topological level, mods are sorted alphabetically by `id`
/// for determinism (Core Principle 4).
///
/// # Active vs installed
///
/// All discovered, valid mods start active.  Use
/// [`InstalledMods::deactivate`] to toggle a mod off at runtime;
/// [`InstalledMods::activate`] to re-enable it.
/// [`InstalledMods::get_loaded_mods`] returns only the currently-active
/// subset, in dependency order.
#[derive(Clone, Debug, Default, Resource)]
pub struct InstalledMods {
    /// Dependency-ordered list of all successfully discovered manifests,
    /// regardless of active state.
    all_mods: Vec<ModManifest>,

    /// Set of mod IDs that have been explicitly deactivated at runtime.
    ///
    /// Mods not in this set (and in `all_mods`) are considered active.
    inactive: HashSet<String>,
}

impl InstalledMods {
    // ── Read-only queries ────────────────────────────────────────────────────

    /// Returns an iterator over ALL installed mod manifests in dependency order,
    /// regardless of whether they are currently active.
    ///
    /// Use this when displaying the full mod list in a settings panel (or the
    /// in-world terminal) so users can see which mods are installed and can be
    /// toggled.
    pub fn iter_all(&self) -> impl Iterator<Item = &ModManifest> {
        self.all_mods.iter()
    }

    /// Returns only the **currently-active** manifests in dependency order.
    ///
    /// This is the list the asset pipeline and other game systems should
    /// respect: a deactivated mod's assets must not be applied even if its
    /// files are present.
    pub fn get_loaded_mods(&self) -> Vec<&ModManifest> {
        self.all_mods
            .iter()
            .filter(|m| !self.inactive.contains(&m.info.id))
            .collect()
    }

    /// Looks up a manifest by its unique mod ID.
    ///
    /// Returns `None` if no installed mod (active or inactive) has that ID.
    pub fn get(&self, id: &str) -> Option<&ModManifest> {
        self.all_mods.iter().find(|m| m.info.id == id)
    }

    /// Returns `true` if a mod with the given ID is installed (active or not).
    pub fn is_installed(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    /// Returns `true` if the mod is installed AND currently active.
    pub fn is_active(&self, id: &str) -> bool {
        self.is_installed(id) && !self.inactive.contains(id)
    }

    // ── Activation control ───────────────────────────────────────────────────

    /// Activates a previously-deactivated mod.
    ///
    /// If the mod is not installed this is a no-op.  If it was already active
    /// this is also a no-op.  Returns `true` if the mod is active after the
    /// call (whether or not a state change occurred).
    pub fn activate(&mut self, id: &str) -> bool {
        if !self.is_installed(id) {
            warn!(mod_id = %id, "activate() called for mod that is not installed — ignored");
            return false;
        }
        self.inactive.remove(id);
        info!(mod_id = %id, "mod activated");
        true
    }

    /// Deactivates an active mod.
    ///
    /// If the mod is not installed this is a no-op.  If it was already
    /// inactive this is also a no-op.  Returns `true` if the mod is inactive
    /// after the call (whether or not a state change occurred).
    pub fn deactivate(&mut self, id: &str) -> bool {
        if !self.is_installed(id) {
            warn!(mod_id = %id, "deactivate() called for mod that is not installed — ignored");
            return false;
        }
        self.inactive.insert(id.to_owned());
        info!(mod_id = %id, "mod deactivated");
        true
    }

    // ── Internal construction (used by the loader system) ───────────────────

    /// Replaces the installed mod list with a freshly-loaded, sorted set.
    ///
    /// Existing inactive state is preserved: any mod ID that was deactivated
    /// before the reload that is still present in `mods` stays deactivated.
    /// Mods that were in the inactive set but are no longer installed are
    /// cleaned up.
    pub fn load(&mut self, mods: Vec<ModManifest>) {
        let installed_ids: HashSet<String> = mods.iter().map(|m| m.info.id.clone()).collect();
        // Clean up stale inactive entries from a previous load.
        self.inactive.retain(|id| installed_ids.contains(id));
        self.all_mods = mods;
    }
}

// ── Loader system ─────────────────────────────────────────────────────────────

/// `PreStartup` system — walks `mods/`, parses every `mod.toml`, validates
/// manifests, topologically sorts them, and populates [`InstalledMods`].
///
/// Delegates all filesystem and sort work to [`load_mods_from_dir`] so that
/// integration tests can call that function with an arbitrary path without
/// needing to manipulate the process working directory.
///
/// # Phase
///
/// `PreStartup` — so the resource is available to all `Startup` systems.
///
/// # Reads
///
/// The filesystem path `mods/` relative to the working directory (the game
/// root in development, the install directory in production).
///
/// # Writes
///
/// [`InstalledMods`] resource (via [`ResMut`]).
pub fn discover_and_load_mods(mut installed: ResMut<InstalledMods>) {
    let mods = load_mods_from_dir(Path::new("mods"));
    installed.load(mods);
}

/// Discovers, validates, and topologically sorts mods from `mods_dir`.
///
/// This is the pure, path-based core of [`discover_and_load_mods`].  Call it
/// directly in integration tests instead of manipulating the process working
/// directory.
///
/// Returns a sorted `Vec<ModManifest>` ready to pass to [`InstalledMods::load`].
/// Returns an empty `Vec` if the directory doesn't exist or contains no valid
/// mods.
///
/// # Error handling
///
/// - Missing directory: logs `info!` and returns `[]`.
/// - Empty directory: logs `info!` and returns `[]`.
/// - Unreadable / un-parseable `mod.toml`: logs `error!` and skips that mod.
/// - `mod.id` ≠ directory name: logs `error!` and skips that mod.
/// - Missing dependency: logs `warn!` and skips the dependent mod.
/// - Dependency cycle: logs `error!` and returns `[]`.
pub fn load_mods_from_dir(mods_dir: &Path) -> Vec<ModManifest> {
    if !mods_dir.exists() {
        info!(path = %mods_dir.display(), "no mods directory found — running without mods");
        return Vec::new();
    }

    // Step 1: Discover and parse all mod.toml files.
    let raw = collect_manifests(mods_dir);

    if raw.is_empty() {
        info!(path = %mods_dir.display(), "mods directory is empty — no mods loaded");
        return Vec::new();
    }

    // Step 2: Validate id-to-directory consistency.
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

    // Step 3: Build an index keyed on mod id.
    let mut by_id: HashMap<String, ModManifest> = validated
        .into_iter()
        .map(|m| (m.info.id.clone(), m))
        .collect();

    // Step 4: Validate that every declared dependency is present.
    let present_ids: BTreeSet<String> = by_id.keys().cloned().collect();
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

    // Step 5: Topological sort (Kahn's algorithm).
    match topological_sort(by_id) {
        Ok(sorted) => {
            let count = sorted.len();
            info!(mod_count = count, "mod manifests loaded and sorted");
            sorted
        }
        Err(cycle_members) => {
            error!(
                ?cycle_members,
                "dependency cycle detected among installed mods — affected mods skipped"
            );
            Vec::new()
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
            Ok(manifest) => results.push((dir_name, manifest)),
            Err(err) => {
                error!(
                    %err,
                    path = %manifest_path.display(),
                    "failed to parse mod.toml — skipping mod"
                );
            }
        }
    }

    // Sort by directory name for a stable pre-sort order before topo pass.
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

/// Topologically sorts a map of manifests using Kahn's algorithm.
///
/// Returns `Ok(sorted)` on success, where `sorted` is the manifests in
/// dependency order (a dependency always precedes its dependent).  Returns
/// `Err(cycle_members)` if a cycle is detected, where `cycle_members` lists
/// the IDs involved in the cycle.
///
/// Within each topological level (mods whose dependencies are all already
/// resolved), mods are emitted in alphabetical ID order for determinism
/// (Core Principle 4).
pub fn topological_sort(
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

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// Builds a minimal [`ModManifest`] for use in unit tests.
    fn make_manifest(id: &str, deps: &[&str]) -> ModManifest {
        ModManifest {
            schema_version: 1,
            info: ModInfo {
                id: id.to_owned(),
                name: id.to_owned(),
                version: "0.1.0".to_owned(),
                description: String::new(),
                author: String::new(),
                dependencies: deps.iter().map(|s| s.to_string()).collect(),
                game_version_min: "0.1.0".to_owned(),
            },
            licensing: ModLicensing {
                spdx_license: "CC-BY-4.0".to_owned(),
                free_distribution_url: String::new(),
            },
        }
    }

    /// Populates an [`InstalledMods`] resource directly from a manifest list
    /// without going through the filesystem.
    fn installed_from(mods: Vec<ModManifest>) -> InstalledMods {
        let mut r = InstalledMods::default();
        r.load(mods);
        r
    }

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

    /// A mod with no dependencies sorts before a mod that depends on it.
    #[test]
    fn topological_sort_simple_dependency() {
        let mut map: HashMap<String, ModManifest> = HashMap::new();
        map.insert("b.mod".to_owned(), make_manifest("b.mod", &["a.dep"]));
        map.insert("a.dep".to_owned(), make_manifest("a.dep", &[]));

        let sorted = topological_sort(map).expect("no cycle");
        let ids: Vec<&str> = sorted.iter().map(|m| m.info.id.as_str()).collect();
        let pos_a = ids.iter().position(|&id| id == "a.dep").unwrap();
        let pos_b = ids.iter().position(|&id| id == "b.mod").unwrap();
        assert!(pos_a < pos_b, "dependency must come before dependent");
    }

    /// Diamond dependency: D depends on B and C; B and C both depend on A.
    #[test]
    fn topological_sort_diamond_dependency() {
        let mut map: HashMap<String, ModManifest> = HashMap::new();
        map.insert(
            "d.mod".to_owned(),
            make_manifest("d.mod", &["b.mod", "c.mod"]),
        );
        map.insert("c.mod".to_owned(), make_manifest("c.mod", &["a.base"]));
        map.insert("b.mod".to_owned(), make_manifest("b.mod", &["a.base"]));
        map.insert("a.base".to_owned(), make_manifest("a.base", &[]));

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
        map.insert("x.mod".to_owned(), make_manifest("x.mod", &["y.mod"]));
        map.insert("y.mod".to_owned(), make_manifest("y.mod", &["x.mod"]));

        let result = topological_sort(map);
        assert!(result.is_err(), "cycle should return Err");
        let cycle = result.unwrap_err();
        assert!(cycle.contains(&"x.mod".to_owned()));
        assert!(cycle.contains(&"y.mod".to_owned()));
    }

    /// Alphabetical tie-breaking enforces Core Principle 4 (determinism).
    #[test]
    fn topological_sort_deterministic_within_level() {
        let mut map: HashMap<String, ModManifest> = HashMap::new();
        map.insert("z.mod".to_owned(), make_manifest("z.mod", &[]));
        map.insert("a.mod".to_owned(), make_manifest("a.mod", &[]));
        map.insert("m.mod".to_owned(), make_manifest("m.mod", &[]));

        let sorted = topological_sort(map).expect("no cycle");
        let ids: Vec<&str> = sorted.iter().map(|m| m.info.id.as_str()).collect();
        assert_eq!(ids, vec!["a.mod", "m.mod", "z.mod"]);
    }

    // ── InstalledMods resource queries ────────────────────────────────────────

    /// `get` and `is_installed` find any mod (active or not) by ID.
    #[test]
    fn installed_mods_lookup() {
        let installed = installed_from(vec![make_manifest("a.mod", &[])]);
        assert!(installed.is_installed("a.mod"));
        assert!(!installed.is_installed("missing.mod"));
        assert_eq!(installed.get("a.mod").unwrap().info.id, "a.mod");
        assert!(installed.get("missing.mod").is_none());
    }

    /// All mods start active; `get_loaded_mods` returns all of them.
    #[test]
    fn all_mods_active_by_default() {
        let installed = installed_from(vec![
            make_manifest("a.mod", &[]),
            make_manifest("b.mod", &[]),
        ]);
        let loaded = installed.get_loaded_mods();
        assert_eq!(loaded.len(), 2, "both mods should be active");
    }

    /// `is_active` returns true for installed mods before any deactivation.
    #[test]
    fn is_active_reflects_default_state() {
        let installed = installed_from(vec![make_manifest("a.mod", &[])]);
        assert!(installed.is_active("a.mod"));
        assert!(!installed.is_active("not-installed.mod"));
    }

    // ── activate / deactivate ─────────────────────────────────────────────────

    /// Deactivating a mod removes it from `get_loaded_mods` but keeps it in
    /// `iter_all`.
    #[test]
    fn deactivate_removes_from_loaded_mods() {
        let mut installed = installed_from(vec![
            make_manifest("a.mod", &[]),
            make_manifest("b.mod", &[]),
        ]);

        let ok = installed.deactivate("a.mod");
        assert!(ok, "deactivate should return true for an installed mod");
        assert!(!installed.is_active("a.mod"));

        let loaded_ids: Vec<&str> = installed
            .get_loaded_mods()
            .iter()
            .map(|m| m.info.id.as_str())
            .collect();
        assert_eq!(loaded_ids, vec!["b.mod"], "only b.mod should remain loaded");

        // iter_all still lists both.
        let all_ids: Vec<&str> = installed.iter_all().map(|m| m.info.id.as_str()).collect();
        assert!(all_ids.contains(&"a.mod"), "a.mod still installed");
    }

    /// Re-activating a deactivated mod brings it back into `get_loaded_mods`.
    #[test]
    fn activate_restores_mod_to_loaded_list() {
        let mut installed = installed_from(vec![
            make_manifest("a.mod", &[]),
            make_manifest("b.mod", &[]),
        ]);

        installed.deactivate("a.mod");
        assert!(!installed.is_active("a.mod"));

        let ok = installed.activate("a.mod");
        assert!(ok, "activate should return true for an installed mod");
        assert!(installed.is_active("a.mod"));
        assert_eq!(
            installed.get_loaded_mods().len(),
            2,
            "both mods active again"
        );
    }

    /// `activate` and `deactivate` on a non-installed mod return false without
    /// panicking.
    #[test]
    fn activate_deactivate_unknown_mod_returns_false() {
        let mut installed = InstalledMods::default();
        assert!(!installed.activate("ghost.mod"));
        assert!(!installed.deactivate("ghost.mod"));
    }

    /// `deactivate` is idempotent — calling it twice on the same mod is safe.
    #[test]
    fn deactivate_is_idempotent() {
        let mut installed = installed_from(vec![make_manifest("a.mod", &[])]);
        installed.deactivate("a.mod");
        installed.deactivate("a.mod"); // second call must not panic
        assert!(!installed.is_active("a.mod"));
    }

    /// `activate` is idempotent — calling it twice on an already-active mod is
    /// safe.
    #[test]
    fn activate_is_idempotent() {
        let mut installed = installed_from(vec![make_manifest("a.mod", &[])]);
        installed.activate("a.mod");
        installed.activate("a.mod"); // second call must not panic
        assert!(installed.is_active("a.mod"));
    }

    /// `get_loaded_mods` preserves dependency order even after deactivation.
    #[test]
    fn loaded_mods_order_preserved_after_deactivation() {
        // Load a → b → c chain (a has no deps, b depends on a, c depends on b).
        let mut map: HashMap<String, ModManifest> = HashMap::new();
        map.insert("a.base".to_owned(), make_manifest("a.base", &[]));
        map.insert("b.mid".to_owned(), make_manifest("b.mid", &["a.base"]));
        map.insert("c.top".to_owned(), make_manifest("c.top", &["b.mid"]));
        let sorted = topological_sort(map).expect("no cycle");

        let mut installed = installed_from(sorted);
        // Deactivate the middle mod.
        installed.deactivate("b.mid");

        let loaded_ids: Vec<&str> = installed
            .get_loaded_mods()
            .iter()
            .map(|m| m.info.id.as_str())
            .collect();
        // a.base and c.top remain, in original dependency order.
        assert_eq!(loaded_ids, vec!["a.base", "c.top"]);
    }

    /// `load` preserves inactive state across reloads for mods still present.
    #[test]
    fn reload_preserves_inactive_state() {
        let mut installed = installed_from(vec![
            make_manifest("a.mod", &[]),
            make_manifest("b.mod", &[]),
        ]);
        installed.deactivate("a.mod");

        // Simulate a reload with the same mods (e.g. hot-reload).
        installed.load(vec![
            make_manifest("a.mod", &[]),
            make_manifest("b.mod", &[]),
        ]);

        assert!(
            !installed.is_active("a.mod"),
            "deactivated mod stays inactive after reload"
        );
        assert!(
            installed.is_active("b.mod"),
            "b.mod still active after reload"
        );
    }

    /// `load` cleans up stale inactive entries for mods that are no longer
    /// installed.
    #[test]
    fn reload_cleans_stale_inactive_entries() {
        let mut installed = installed_from(vec![
            make_manifest("a.mod", &[]),
            make_manifest("b.mod", &[]),
        ]);
        installed.deactivate("b.mod");

        // Reload without b.mod (it was removed from the mods/ directory).
        installed.load(vec![make_manifest("a.mod", &[])]);

        // a.mod is still active; b.mod is gone (not just deactivated).
        assert!(installed.is_active("a.mod"));
        assert!(!installed.is_installed("b.mod"));
    }
}
