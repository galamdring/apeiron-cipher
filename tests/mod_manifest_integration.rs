//! Integration tests for [`apeiron_cipher::mod_manifest`].
//!
//! Each test creates a hermetic temporary directory that mirrors the real
//! `mods/` layout (`mods/<mod_id>/mod.toml`), then calls
//! [`apeiron_cipher::mod_manifest::load_mods_from_dir`] directly —
//! no cwd manipulation, no race conditions, no Bevy `App` overhead for
//! pure logic tests.
//!
//! The Bevy `PreStartup` plugin-wiring test at the bottom uses a minimal
//! headless `App` with `ModManifestPlugin` — it tests the wiring only, not
//! the filesystem traversal (which is covered by the direct-call tests).

use std::{collections::HashMap, fs};

use tempfile::TempDir;

use apeiron_cipher::mod_manifest::{
    InstalledMods, ModInfo, ModLicensing, ModManifest, load_mods_from_dir, topological_sort,
};

// ── Fixtures ─────────────────────────────────────────────────────────────────

/// Writes a minimal valid `mod.toml` into `<root>/<mod_id>/mod.toml`.
fn write_mod_toml(root: &TempDir, mod_id: &str, dependencies: &[&str]) {
    let dir = root.path().join(mod_id);
    fs::create_dir_all(&dir).unwrap();

    let deps_toml = dependencies
        .iter()
        .map(|d| format!("\"{d}\""))
        .collect::<Vec<_>>()
        .join(", ");

    let content = format!(
        r#"schema_version = 1

[mod]
id               = "{mod_id}"
name             = "{mod_id}"
version          = "0.1.0"
description      = "Integration test mod."
author           = "Test"
dependencies     = [{deps_toml}]
game_version_min = "0.1.0"

[licensing]
spdx_license          = "CC-BY-4.0"
free_distribution_url = ""
"#,
    );

    fs::write(dir.join("mod.toml"), content).unwrap();
}

/// Builds a minimal `ModManifest` for unit-level resource testing (no I/O).
fn manifest(id: &str, deps: &[&str]) -> ModManifest {
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

/// Wraps a `Vec<ModManifest>` in an `InstalledMods` resource for API tests.
fn installed_from_sorted(mods: Vec<ModManifest>) -> InstalledMods {
    let mut r = InstalledMods::default();
    r.load(mods);
    r
}

// ── Manifest I/O tests ────────────────────────────────────────────────────────

/// A manifest written to disk round-trips through TOML correctly.
#[test]
fn filesystem_manifest_parses_correctly() {
    let root = TempDir::new().unwrap();
    write_mod_toml(&root, "author.signal-mod", &["base.core"]);

    // Parse manually (mirrors what collect_manifests does internally).
    let path = root.path().join("author.signal-mod").join("mod.toml");
    let contents = fs::read_to_string(&path).unwrap();
    let parsed: ModManifest = toml::from_str(&contents).unwrap();

    assert_eq!(parsed.info.id, "author.signal-mod");
    assert_eq!(parsed.info.dependencies, vec!["base.core"]);
    assert_eq!(parsed.schema_version, 1);
    assert_eq!(parsed.licensing.spdx_license, "CC-BY-4.0");
}

/// A mod with no `dependencies` field still parses (optional).
#[test]
fn manifest_without_dependencies_field_parses() {
    let root = TempDir::new().unwrap();
    let dir = root.path().join("no-deps.mod");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("mod.toml"),
        r#"schema_version = 1

[mod]
id               = "no-deps.mod"
name             = "NoDeps"
version          = "0.1.0"
game_version_min = "0.1.0"

[licensing]
spdx_license = "MIT"
"#,
    )
    .unwrap();

    let contents = fs::read_to_string(dir.join("mod.toml")).unwrap();
    let parsed: ModManifest = toml::from_str(&contents).unwrap();
    assert!(parsed.info.dependencies.is_empty());
    assert_eq!(parsed.licensing.free_distribution_url, "");
}

/// A broken mod.toml returns a parse error.
#[test]
fn malformed_manifest_fails_to_parse() {
    let broken = "this is not valid toml [[[";
    let result: Result<ModManifest, _> = toml::from_str(broken);
    assert!(result.is_err(), "broken toml should not parse");
}

// ── Topological sort tests ────────────────────────────────────────────────────

/// Three-level chain: a → b → c sorts as [a, b, c].
#[test]
fn topological_sort_three_level_chain() {
    let mut map = HashMap::new();
    map.insert("c.top".to_owned(), manifest("c.top", &["b.mid"]));
    map.insert("b.mid".to_owned(), manifest("b.mid", &["a.base"]));
    map.insert("a.base".to_owned(), manifest("a.base", &[]));

    let sorted = topological_sort(map).expect("no cycle expected");
    let ids: Vec<&str> = sorted.iter().map(|m| m.info.id.as_str()).collect();

    let pos = |id: &str| ids.iter().position(|&x| x == id).unwrap();
    assert!(pos("a.base") < pos("b.mid"));
    assert!(pos("b.mid") < pos("c.top"));
}

/// Five independent mods sort alphabetically (Core Principle 4 — determinism).
#[test]
fn topological_sort_five_independent_mods_alphabetical() {
    let ids_in = [
        "echo.mod",
        "alpha.mod",
        "delta.mod",
        "bravo.mod",
        "charlie.mod",
    ];
    let mut map = HashMap::new();
    for id in ids_in {
        map.insert(id.to_owned(), manifest(id, &[]));
    }

    let sorted = topological_sort(map).expect("no cycle");
    let ids_out: Vec<&str> = sorted.iter().map(|m| m.info.id.as_str()).collect();
    assert_eq!(
        ids_out,
        vec![
            "alpha.mod",
            "bravo.mod",
            "charlie.mod",
            "delta.mod",
            "echo.mod"
        ]
    );
}

/// Single mod sorts to a one-element list.
#[test]
fn topological_sort_single_mod() {
    let mut map = HashMap::new();
    map.insert("solo.mod".to_owned(), manifest("solo.mod", &[]));

    let sorted = topological_sort(map).expect("no cycle");
    assert_eq!(sorted.len(), 1);
    assert_eq!(sorted[0].info.id, "solo.mod");
}

/// Empty input → empty output.
#[test]
fn topological_sort_empty_input() {
    let sorted = topological_sort(HashMap::new()).expect("no cycle");
    assert!(sorted.is_empty());
}

/// Binary cycle returns `Err` with both IDs.
#[test]
fn topological_sort_binary_cycle_returns_err() {
    let mut map = HashMap::new();
    map.insert("p.mod".to_owned(), manifest("p.mod", &["q.mod"]));
    map.insert("q.mod".to_owned(), manifest("q.mod", &["p.mod"]));

    let result = topological_sort(map);
    assert!(result.is_err());
    let cycle = result.unwrap_err();
    assert_eq!(cycle.len(), 2);
    assert!(cycle.contains(&"p.mod".to_owned()));
    assert!(cycle.contains(&"q.mod".to_owned()));
}

/// Three-way cycle is detected and all members returned.
#[test]
fn topological_sort_three_way_cycle() {
    let mut map = HashMap::new();
    map.insert("p.mod".to_owned(), manifest("p.mod", &["r.mod"]));
    map.insert("q.mod".to_owned(), manifest("q.mod", &["p.mod"]));
    map.insert("r.mod".to_owned(), manifest("r.mod", &["q.mod"]));

    let result = topological_sort(map);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().len(), 3);
}

// ── InstalledMods resource tests ──────────────────────────────────────────────

/// Default resource is empty.
#[test]
fn installed_mods_default_is_empty() {
    let installed = InstalledMods::default();
    assert_eq!(installed.iter_all().count(), 0);
    assert_eq!(installed.get_loaded_mods().len(), 0);
    assert!(!installed.is_installed("any.mod"));
    assert!(!installed.is_active("any.mod"));
}

/// Deactivating an unknown mod is a safe no-op.
#[test]
fn deactivate_unknown_mod_safe() {
    let mut installed = InstalledMods::default();
    assert!(!installed.deactivate("ghost.mod"));
}

/// Activating an unknown mod is a safe no-op.
#[test]
fn activate_unknown_mod_safe() {
    let mut installed = InstalledMods::default();
    assert!(!installed.activate("ghost.mod"));
}

// ── load_mods_from_dir (filesystem-backed) ────────────────────────────────────

/// No `mods/` directory → empty result.
#[test]
fn no_mods_dir_produces_empty_result() {
    let root = TempDir::new().unwrap();
    // Do NOT create a mods/ subdirectory.
    let mods = load_mods_from_dir(&root.path().join("mods"));
    assert!(mods.is_empty());
}

/// Empty `mods/` directory → empty result.
#[test]
fn empty_mods_dir_produces_empty_result() {
    let root = TempDir::new().unwrap();
    let mods_dir = root.path().join("mods");
    fs::create_dir(&mods_dir).unwrap();

    let mods = load_mods_from_dir(&mods_dir);
    assert!(mods.is_empty());
}

/// A single valid mod is discovered.
#[test]
fn single_valid_mod_is_discovered() {
    let root = TempDir::new().unwrap();
    let mods_dir = root.path().join("mods");
    fs::create_dir(&mods_dir).unwrap();

    let mod_dir = mods_dir.join("author.solo-mod");
    fs::create_dir_all(&mod_dir).unwrap();
    fs::write(
        mod_dir.join("mod.toml"),
        r#"schema_version = 1

[mod]
id               = "author.solo-mod"
name             = "Solo Mod"
version          = "0.1.0"
game_version_min = "0.1.0"

[licensing]
spdx_license = "CC-BY-4.0"
"#,
    )
    .unwrap();

    let mods = load_mods_from_dir(&mods_dir);
    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].info.id, "author.solo-mod");
}

/// A mod whose `mod.id` mismatches its directory name is skipped.
#[test]
fn mod_id_mismatch_with_dir_name_skipped() {
    let root = TempDir::new().unwrap();
    let mods_dir = root.path().join("mods");
    fs::create_dir_all(&mods_dir).unwrap();

    // Directory name = "wrong.dir", but mod.id = "correct.id"
    let mod_dir = mods_dir.join("wrong.dir");
    fs::create_dir_all(&mod_dir).unwrap();
    fs::write(
        mod_dir.join("mod.toml"),
        r#"schema_version = 1

[mod]
id               = "correct.id"
name             = "Misnamed Mod"
version          = "0.1.0"
game_version_min = "0.1.0"

[licensing]
spdx_license = "MIT"
"#,
    )
    .unwrap();

    let mods = load_mods_from_dir(&mods_dir);
    assert!(mods.is_empty(), "mismatched mod should be skipped");
}

/// A mod with a missing dependency is skipped; independent mods load fine.
#[test]
fn mod_with_missing_dependency_is_skipped() {
    let root = TempDir::new().unwrap();
    let mods_dir = root.path().join("mods");
    fs::create_dir_all(&mods_dir).unwrap();

    // independent.mod has no deps — should load.
    let ok_dir = mods_dir.join("independent.mod");
    fs::create_dir_all(&ok_dir).unwrap();
    fs::write(
        ok_dir.join("mod.toml"),
        r#"schema_version = 1
[mod]
id = "independent.mod"
name = "Independent"
version = "0.1.0"
game_version_min = "0.1.0"
[licensing]
spdx_license = "MIT"
"#,
    )
    .unwrap();

    // dependent.mod needs missing.dep — should be skipped.
    let dep_dir = mods_dir.join("dependent.mod");
    fs::create_dir_all(&dep_dir).unwrap();
    fs::write(
        dep_dir.join("mod.toml"),
        r#"schema_version = 1
[mod]
id           = "dependent.mod"
name         = "Dependent"
version      = "0.1.0"
dependencies = ["missing.dep"]
game_version_min = "0.1.0"
[licensing]
spdx_license = "MIT"
"#,
    )
    .unwrap();

    let mods = load_mods_from_dir(&mods_dir);
    assert_eq!(mods.len(), 1, "only independent.mod should load");
    assert_eq!(mods[0].info.id, "independent.mod");
}

/// Two mods with a declared dependency load in correct order.
#[test]
fn two_mods_dependency_order_preserved() {
    let root = TempDir::new().unwrap();
    let mods_dir = root.path().join("mods");
    fs::create_dir_all(&mods_dir).unwrap();

    // base.lib has no deps.
    let base_dir = mods_dir.join("base.lib");
    fs::create_dir_all(&base_dir).unwrap();
    fs::write(
        base_dir.join("mod.toml"),
        r#"schema_version = 1
[mod]
id = "base.lib"
name = "Base Lib"
version = "0.1.0"
game_version_min = "0.1.0"
[licensing]
spdx_license = "MIT"
"#,
    )
    .unwrap();

    // overlay.mod depends on base.lib.
    let overlay_dir = mods_dir.join("overlay.mod");
    fs::create_dir_all(&overlay_dir).unwrap();
    fs::write(
        overlay_dir.join("mod.toml"),
        r#"schema_version = 1
[mod]
id           = "overlay.mod"
name         = "Overlay"
version      = "0.1.0"
dependencies = ["base.lib"]
game_version_min = "0.1.0"
[licensing]
spdx_license = "MIT"
"#,
    )
    .unwrap();

    let mods = load_mods_from_dir(&mods_dir);
    assert_eq!(mods.len(), 2);

    let ids: Vec<&str> = mods.iter().map(|m| m.info.id.as_str()).collect();
    let pos_base = ids.iter().position(|&id| id == "base.lib").unwrap();
    let pos_overlay = ids.iter().position(|&id| id == "overlay.mod").unwrap();
    assert!(pos_base < pos_overlay, "base.lib must precede overlay.mod");
}

/// `load_mods_from_dir` + `InstalledMods::load` + activate/deactivate round-trip.
#[test]
fn activate_deactivate_round_trip() {
    let root = TempDir::new().unwrap();
    let mods_dir = root.path().join("mods");
    fs::create_dir_all(&mods_dir).unwrap();

    for id in ["alpha.mod", "beta.mod"] {
        let mod_dir = mods_dir.join(id);
        fs::create_dir_all(&mod_dir).unwrap();
        fs::write(
            mod_dir.join("mod.toml"),
            format!(
                r#"schema_version = 1
[mod]
id = "{id}"
name = "{id}"
version = "0.1.0"
game_version_min = "0.1.0"
[licensing]
spdx_license = "MIT"
"#
            ),
        )
        .unwrap();
    }

    let mods = load_mods_from_dir(&mods_dir);
    let mut installed = installed_from_sorted(mods);

    assert_eq!(installed.get_loaded_mods().len(), 2, "both start active");

    installed.deactivate("alpha.mod");
    assert_eq!(installed.get_loaded_mods().len(), 1);
    assert_eq!(installed.get_loaded_mods()[0].info.id, "beta.mod");

    installed.activate("alpha.mod");
    assert_eq!(installed.get_loaded_mods().len(), 2, "both active again");
}

/// Diamond dependency loads all four mods in correct order.
#[test]
fn diamond_dependency_loads_all_four_mods() {
    let root = TempDir::new().unwrap();
    let mods_dir = root.path().join("mods");
    fs::create_dir_all(&mods_dir).unwrap();

    // a.base ← b.mid and c.mid ← d.top
    for (id, deps) in [
        ("a.base", vec![]),
        ("b.mid", vec!["a.base"]),
        ("c.mid", vec!["a.base"]),
        ("d.top", vec!["b.mid", "c.mid"]),
    ] {
        write_mod_toml(&TempDir::new().unwrap(), id, &[]); // just for shape reference
        let mod_dir = mods_dir.join(id);
        fs::create_dir_all(&mod_dir).unwrap();
        let deps_toml = deps
            .iter()
            .map(|d| format!("\"{d}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            mod_dir.join("mod.toml"),
            format!(
                r#"schema_version = 1
[mod]
id           = "{id}"
name         = "{id}"
version      = "0.1.0"
dependencies = [{deps_toml}]
game_version_min = "0.1.0"
[licensing]
spdx_license = "MIT"
"#
            ),
        )
        .unwrap();
    }

    let mods = load_mods_from_dir(&mods_dir);
    assert_eq!(mods.len(), 4);

    let ids: Vec<&str> = mods.iter().map(|m| m.info.id.as_str()).collect();
    let pos = |id: &str| ids.iter().position(|&x| x == id).unwrap();

    assert!(pos("a.base") < pos("b.mid"), "a.base before b.mid");
    assert!(pos("a.base") < pos("c.mid"), "a.base before c.mid");
    assert!(pos("b.mid") < pos("d.top"), "b.mid before d.top");
    assert!(pos("c.mid") < pos("d.top"), "c.mid before d.top");
}

// ── Bevy plugin wiring test ───────────────────────────────────────────────────

/// The `ModManifestPlugin` inserts `InstalledMods` as a resource — test the
/// wiring only (no mods/ dir expected in CI working directory).
#[test]
fn plugin_registers_installed_mods_resource() {
    use apeiron_cipher::mod_manifest::ModManifestPlugin;
    use bevy::prelude::*;

    let mut app = App::new();
    app.add_plugins(ModManifestPlugin);
    app.update();

    assert!(
        app.world().get_resource::<InstalledMods>().is_some(),
        "InstalledMods resource must be present after plugin initializes"
    );
}
