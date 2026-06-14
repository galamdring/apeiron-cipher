//! Mod directory discovery and registration.
//!
//! Scans the `mods/` directory at startup, parses each mod's `mod.toml`
//! manifest, and produces a sorted [`ModRegistry`] resource. Downstream asset
//! loaders (`classification`, `world_generation`, `combination`, `exterior`)
//! consume this resource to merge mod contributions into the base-game
//! registries after base assets are loaded.
//!
//! ## Load order
//!
//! 1. Base game assets (unchanged, always first).
//! 2. Mod assets, alphabetically by directory name. Within a mod directory
//!    the asset sub-tree mirrors `assets/` exactly — only the files present
//!    in a mod are considered; the rest of the base tree is untouched.
//!
//! ## Duplicate handling
//!
//! When a later mod provides a record whose identifier matches one already
//! loaded (by the base game or an earlier mod), the later record wins and a
//! `warn!()` is emitted. This makes mod overrides explicit in logs without
//! being fatal.
//!
//! ## Validation
//!
//! A mod directory is included only when its `mod.toml` parses successfully
//! AND the `[mod].id` field equals the directory name. Any other directory
//! (no manifest, malformed manifest, id mismatch) is skipped with a `warn!()`.
//!
//! ## System ordering
//!
//! [`scan_mod_registry`] runs in `PreStartup` inside [`ModScanSet::Scan`].
//! All other `PreStartup` systems that need `Res<ModRegistry>` must be
//! scheduled `.after(ModScanSet::Scan)`. Systems in `Startup` (which runs
//! after all `PreStartup` work) can read `Res<ModRegistry>` freely.

use std::{fs, path::Path, path::PathBuf};

use bevy::prelude::*;
use serde::Deserialize;

/// Directory under the game root that contains installed mod sub-directories.
pub const MODS_DIR: &str = "mods";

/// System set that owns the mod-directory scan.
///
/// Other `PreStartup` systems that merge mod assets reference this set with
/// `.after(ModScanSet::Scan)` to guarantee the scan completes before they
/// attempt to read `Res<ModRegistry>`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModScanSet {
    /// The mod-directory scan that populates [`ModRegistry`].
    Scan,
}

/// A single discovered and validated mod entry.
#[derive(Debug, Clone)]
pub struct LoadedMod {
    /// Globally unique identifier, from `mod.toml → [mod].id`.
    ///
    /// Must equal the directory name — the loader rejects mismatches.
    pub id: String,
    /// Human-readable display name, from `mod.toml → [mod].name`.
    pub name: String,
    /// Absolute path to the mod's root directory.
    pub dir: PathBuf,
}

impl LoadedMod {
    /// The globally unique mod identifier (matches the directory name).
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the root of this mod's `assets/` subtree.
    ///
    /// Downstream asset loaders join sub-paths onto this to locate individual
    /// asset files that mirror the base-game layout.
    pub fn asset_root(&self) -> PathBuf {
        self.dir.join("assets")
    }

    /// Returns the path to an asset file within this mod's `assets/` subtree.
    ///
    /// The path `relative` mirrors the base game's layout — e.g.
    /// `"materials/classifications.toml"` → `<mod_dir>/assets/materials/classifications.toml`.
    pub fn asset_path(&self, relative: &str) -> PathBuf {
        self.dir.join("assets").join(relative)
    }
}

impl ModRegistry {
    /// Returns all discovered mods in alphabetical load order.
    pub fn mods(&self) -> &[LoadedMod] {
        &self.mods
    }
}

/// All discovered and validated mods in alphabetical load order.
///
/// Populated once in `PreStartup` by [`scan_mod_registry`]. Downstream
/// systems consume this as `Res<ModRegistry>` to locate and merge each mod's
/// asset files.
///
/// An empty registry (no `mods/` directory, or no valid mods) is the normal
/// state for a vanilla installation — the game runs without modification.
#[derive(Resource, Debug, Default)]
pub struct ModRegistry {
    /// Mods sorted alphabetically by directory name.
    pub mods: Vec<LoadedMod>,
}

// ── Manifest deserialization ──────────────────────────────────────────────

/// Top-level shape of `mod.toml`.
///
/// `schema_version` must be the first field. Only version `1` is currently
/// supported; future versions may introduce migration logic.
#[derive(Debug, Deserialize)]
struct ModManifest {
    /// Must be `1` for the current format.
    schema_version: u32,
    /// The `[mod]` table.
    #[serde(rename = "mod")]
    meta: ModMeta,
}

/// Contents of the `[mod]` table within `mod.toml`.
#[derive(Debug, Deserialize)]
struct ModMeta {
    /// Globally unique reverse-domain identifier. Must match the directory name.
    id: String,
    /// Display name shown in community tooling and logs.
    name: String,
    /// Semantic version string.
    version: String,
    /// Minimum game version this mod targets. Logged as a warning if not met.
    #[allow(dead_code)]
    game_version_min: String,
}

// ── Plugin ────────────────────────────────────────────────────────────────

/// Registers the mod scan and the [`ModRegistry`] resource.
///
/// **This plugin must be added before all other game plugins** so that
/// `PreStartup` has the `ModScanSet::Scan` set configured before any
/// asset-merging system tries to reference it with `.after(ModScanSet::Scan)`.
pub struct ModRegistryPlugin;

impl Plugin for ModRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModRegistry>()
            .configure_sets(PreStartup, ModScanSet::Scan)
            .add_systems(PreStartup, scan_mod_registry.in_set(ModScanSet::Scan));
    }
}

// ── Systems ───────────────────────────────────────────────────────────────

/// Discovers all valid mod directories under [`MODS_DIR`] and populates
/// [`ModRegistry`].
///
/// Runs in `PreStartup`, inside [`ModScanSet::Scan`], before any system that
/// merges mod assets. Mods are sorted alphabetically so load order is
/// deterministic regardless of OS or file-system implementation.
fn scan_mod_registry(mut registry: ResMut<ModRegistry>) {
    registry.mods = discover_mods(Path::new(MODS_DIR));
}

/// Pure function: scan `mods_dir` for valid mod directories and return them
/// in alphabetical order.
///
/// Extracted from the Bevy system so that unit tests can pass an arbitrary
/// directory without launching a full `App`.
pub fn discover_mods(mods_dir: &Path) -> Vec<LoadedMod> {
    if !mods_dir.exists() {
        info!(
            "Mods directory '{}' not found — no mods loaded",
            mods_dir.display()
        );
        return Vec::new();
    }

    let read_dir = match fs::read_dir(mods_dir) {
        Ok(rd) => rd,
        Err(e) => {
            warn!("Cannot read mods directory '{}': {e}", mods_dir.display());
            return Vec::new();
        }
    };

    // Collect all sub-directories, then sort for deterministic order.
    let mut dirs: Vec<PathBuf> = read_dir
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            path.is_dir().then_some(path)
        })
        .collect();
    dirs.sort();

    let mut mods = Vec::new();

    for dir in dirs {
        let dir_name = match dir.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => {
                warn!(
                    "Mod directory has a non-UTF-8 name — skipped: {}",
                    dir.display()
                );
                continue;
            }
        };

        let manifest_path = dir.join("mod.toml");
        if !manifest_path.exists() {
            warn!("Mod directory '{dir_name}' has no mod.toml — skipped");
            continue;
        }

        let contents = match fs::read_to_string(&manifest_path) {
            Ok(s) => s,
            Err(e) => {
                warn!("Cannot read mod.toml in '{dir_name}': {e} — skipped");
                continue;
            }
        };

        let manifest: ModManifest = match toml::from_str(&contents) {
            Ok(m) => m,
            Err(e) => {
                warn!("Malformed mod.toml in '{dir_name}': {e} — skipped");
                continue;
            }
        };

        if manifest.schema_version != 1 {
            warn!(
                "Mod '{dir_name}' has unsupported schema_version {} (expected 1) — skipped",
                manifest.schema_version
            );
            continue;
        }

        // The directory name must be identical to the declared mod ID.
        // This prevents renaming a mod directory without updating the manifest.
        if manifest.meta.id != dir_name {
            warn!(
                "Mod manifest id='{}' does not match directory name '{dir_name}' — skipped",
                manifest.meta.id
            );
            continue;
        }

        info!(
            "Discovered mod: {} v{} (id: {dir_name})",
            manifest.meta.name, manifest.meta.version
        );

        mods.push(LoadedMod {
            id: manifest.meta.id,
            name: manifest.meta.name,
            dir,
        });
    }

    info!("Mod scan complete: {} mod(s) discovered", mods.len());
    mods
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// Build a minimal but valid `mod.toml` contents string.
    fn valid_manifest(id: &str, name: &str) -> String {
        format!(
            r#"schema_version = 1

[mod]
id               = "{id}"
name             = "{name}"
version          = "1.0.0"
game_version_min = "0.1.0"

[licensing]
spdx_license          = "CC-BY-4.0"
free_distribution_url = ""
"#
        )
    }

    /// Create a minimal mod directory under `parent` with the given `id`.
    fn make_mod_dir(parent: &Path, id: &str, name: &str) -> PathBuf {
        let dir = parent.join(id);
        fs::create_dir_all(&dir).expect("create mod dir");
        fs::write(dir.join("mod.toml"), valid_manifest(id, name)).expect("write mod.toml");
        dir
    }

    #[test]
    fn empty_mods_dir_returns_no_mods() {
        let tmp = std::env::temp_dir().join("apeiron_test_empty_mods");
        fs::create_dir_all(&tmp).expect("create tmp dir");
        // empty directory
        let result = discover_mods(&tmp);
        assert!(result.is_empty(), "expected no mods, got {:?}", result);
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn missing_mods_dir_returns_no_mods() {
        let nonexistent = Path::new("/tmp/apeiron_does_not_exist_xyz_123");
        let result = discover_mods(nonexistent);
        assert!(result.is_empty());
    }

    #[test]
    fn single_valid_mod_is_discovered() {
        let tmp = std::env::temp_dir().join("apeiron_test_single_mod");
        fs::create_dir_all(&tmp).expect("create tmp dir");
        make_mod_dir(&tmp, "test.alpha", "Alpha Mod");

        let result = discover_mods(&tmp);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "test.alpha");
        assert_eq!(result[0].name, "Alpha Mod");

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn mods_are_sorted_alphabetically() {
        let tmp = std::env::temp_dir().join("apeiron_test_sort_mods");
        fs::create_dir_all(&tmp).expect("create tmp dir");
        // Create in reverse order to verify sorting.
        make_mod_dir(&tmp, "test.gamma", "Gamma");
        make_mod_dir(&tmp, "test.alpha", "Alpha");
        make_mod_dir(&tmp, "test.beta", "Beta");

        let result = discover_mods(&tmp);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].id, "test.alpha");
        assert_eq!(result[1].id, "test.beta");
        assert_eq!(result[2].id, "test.gamma");

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn mod_missing_mod_toml_is_skipped() {
        let tmp = std::env::temp_dir().join("apeiron_test_no_manifest");
        fs::create_dir_all(&tmp).expect("create tmp dir");
        // A directory with NO mod.toml.
        fs::create_dir_all(tmp.join("no.manifest")).expect("create mod dir");
        // A valid mod alongside it.
        make_mod_dir(&tmp, "valid.mod", "Valid");

        let result = discover_mods(&tmp);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "valid.mod");

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn mod_with_id_mismatch_is_skipped() {
        let tmp = std::env::temp_dir().join("apeiron_test_id_mismatch");
        fs::create_dir_all(&tmp).expect("create tmp dir");
        // The directory is called "dir.name" but manifest says id = "other.name".
        let dir = tmp.join("dir.name");
        fs::create_dir_all(&dir).expect("create mod dir");
        fs::write(
            dir.join("mod.toml"),
            valid_manifest("other.name", "Mismatch"),
        )
        .expect("write mod.toml");

        let result = discover_mods(&tmp);
        assert!(
            result.is_empty(),
            "expected id-mismatch mod to be skipped, got {:?}",
            result
        );

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn mod_with_malformed_manifest_is_skipped() {
        let tmp = std::env::temp_dir().join("apeiron_test_malformed");
        fs::create_dir_all(&tmp).expect("create tmp dir");
        let dir = tmp.join("bad.mod");
        fs::create_dir_all(&dir).expect("create mod dir");
        fs::write(dir.join("mod.toml"), "this is not valid toml ][").expect("write bad mod.toml");

        let result = discover_mods(&tmp);
        assert!(result.is_empty());

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn asset_path_mirrors_assets_layout() {
        let tmp = std::env::temp_dir().join("apeiron_test_asset_path");
        fs::create_dir_all(&tmp).expect("create tmp dir");
        make_mod_dir(&tmp, "path.test", "Path Test");

        let mods = discover_mods(&tmp);
        assert_eq!(mods.len(), 1);

        let expected = tmp
            .join("path.test")
            .join("assets")
            .join("materials")
            .join("classifications.toml");
        let actual = mods[0].asset_path("materials/classifications.toml");
        assert_eq!(actual, expected);

        fs::remove_dir_all(&tmp).ok();
    }
}
