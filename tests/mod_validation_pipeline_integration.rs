//! Integration tests for the mod validation pipeline (Epic 23, Story 23.1,
//! task t_d775c346).
//!
//! These tests exercise the full lifecycle: filesystem fixtures → manifest
//! parsing → validation via `ModValidator` → activation decisions via
//! `InstalledMods::validate_and_activate`.
//!
//! All tests use hermetic `TempDir` fixtures — no working-directory
//! manipulation, no global state, no Bevy `App` overhead for pure logic.

use std::fs;

use tempfile::TempDir;

use apeiron_cipher::{
    mod_manifest::{InstalledMods, ModInfo, ModLicensing, ModManifest, load_mods_from_dir},
    mod_validator::ModValidator,
};

// ── Fixture helpers ───────────────────────────────────────────────────────────

/// Writes a complete, valid `mod.toml` into `<root>/<mod_id>/mod.toml`.
fn write_mod(root: &TempDir, mod_id: &str, deps: &[&str]) {
    write_mod_with_version(root, mod_id, "0.1.0", deps);
}

/// Writes a `mod.toml` with a custom `game_version_min`.
fn write_mod_with_version(root: &TempDir, mod_id: &str, game_version_min: &str, deps: &[&str]) {
    let dir = root.path().join(mod_id);
    fs::create_dir_all(&dir).unwrap();

    let deps_toml = deps
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
game_version_min = "{game_version_min}"

[licensing]
spdx_license          = "CC-BY-4.0"
free_distribution_url = ""
"#,
    );

    fs::write(dir.join("mod.toml"), content).unwrap();
}

/// Builds a minimal, valid `ModManifest` in memory (no filesystem).
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

/// Wraps a list of manifests in an `InstalledMods` resource.
fn installed_from(mods: Vec<ModManifest>) -> InstalledMods {
    let mut r = InstalledMods::default();
    r.load(mods);
    r
}

// ── 1. Schema validation ──────────────────────────────────────────────────────

/// A well-formed manifest with all required fields passes schema validation.
#[test]
fn valid_manifest_passes_schema_validation() {
    let m = manifest("author.good-mod", &[]);
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, None);
    assert!(result.is_valid(), "unexpected errors: {:?}", result.errors);
}

/// `schema_version = 2` is rejected — only version 1 is understood.
#[test]
fn unknown_schema_version_is_rejected() {
    let mut m = manifest("author.my-mod", &[]);
    m.schema_version = 2;
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, None);
    assert!(!result.is_valid());
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("schema_version 2"))
    );
}

/// An empty `mod.id` produces an error.
#[test]
fn empty_id_field_is_rejected() {
    let mut m = manifest("author.my-mod", &[]);
    m.info.id = String::new();
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, None);
    assert!(!result.is_valid());
    assert!(result.errors.iter().any(|e| e.message.contains("mod.id")));
}

/// An `id` lacking a dot (no `author.slug` form) is rejected.
#[test]
fn id_without_dot_notation_is_rejected() {
    let mut m = manifest("nodotmod", &[]);
    m.info.id = "nodotmod".to_owned();
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, None);
    assert!(!result.is_valid());
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("dot-notation"))
    );
}

/// A missing `spdx_license` value is a schema error.
#[test]
fn empty_spdx_license_is_rejected() {
    let mut m = manifest("author.my-mod", &[]);
    m.licensing.spdx_license = String::new();
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, None);
    assert!(!result.is_valid());
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("spdx_license"))
    );
}

// ── 2. Version parsing ────────────────────────────────────────────────────────

/// A non-semver `version` field is rejected.
#[test]
fn malformed_version_field_is_rejected() {
    let mut m = manifest("author.my-mod", &[]);
    m.info.version = "1.0".to_owned(); // two parts, not three
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, None);
    assert!(!result.is_valid());
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("mod.version"))
    );
}

/// A non-semver `game_version_min` is rejected.
#[test]
fn malformed_game_version_min_is_rejected() {
    let mut m = manifest("author.my-mod", &[]);
    m.info.game_version_min = "not-semver".to_owned();
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, None);
    assert!(!result.is_valid());
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("game_version_min"))
    );
}

// ── 3. Dependency resolution ──────────────────────────────────────────────────

/// A mod whose dependencies are all present passes validation.
#[test]
fn satisfied_dependencies_pass() {
    let m = manifest("author.mod-b", &["author.mod-a"]);
    let validator = ModValidator::new("0.1.0", &["author.mod-a"]);
    let result = validator.validate(&m, None);
    assert!(result.is_valid(), "unexpected errors: {:?}", result.errors);
}

/// A mod declaring a dependency that is not installed is rejected.
#[test]
fn missing_dependency_blocks_validation() {
    let m = manifest("author.mod-b", &["author.missing-dep"]);
    let validator = ModValidator::new("0.1.0", &[]); // dep not installed
    let result = validator.validate(&m, None);
    assert!(!result.is_valid());
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("author.missing-dep"))
    );
}

/// Each missing dependency produces its own distinct error.
#[test]
fn multiple_missing_deps_each_produce_an_error() {
    let m = manifest("author.mod", &["a.dep", "b.dep", "c.dep"]);
    let validator = ModValidator::new("0.1.0", &["a.dep"]); // b.dep, c.dep absent
    let result = validator.validate(&m, None);
    assert!(!result.is_valid());
    // Expect exactly 2 errors (b.dep and c.dep).
    let dep_errors: Vec<&str> = result
        .errors
        .iter()
        .map(|e| e.message.as_str())
        .filter(|m| m.contains(".dep"))
        .collect();
    assert_eq!(
        dep_errors.len(),
        2,
        "expected 2 dep errors, got: {:?}",
        dep_errors
    );
}

/// A mod with no dependencies always passes the dependency check.
#[test]
fn no_dependencies_always_passes() {
    let m = manifest("author.standalone", &[]);
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, None);
    assert!(result.is_valid());
}

// ── 4. Entry-point existence ──────────────────────────────────────────────────

/// Supplying a valid mod_root dir (with mod.toml) passes the entry-point check.
#[test]
fn valid_mod_root_passes_entry_point_check() {
    let tmp = TempDir::new().unwrap();
    let mod_dir = tmp.path().join("author.my-mod");
    fs::create_dir_all(&mod_dir).unwrap();
    fs::write(mod_dir.join("mod.toml"), "# stub").unwrap();

    let m = manifest("author.my-mod", &[]);
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, Some(&mod_dir));
    assert!(result.is_valid(), "unexpected errors: {:?}", result.errors);
}

/// A mod_root path that does not exist on disk is a hard error.
#[test]
fn nonexistent_mod_root_is_rejected() {
    let fake = std::path::Path::new("/tmp/surely_absent_path_abc123/author.my-mod");
    let m = manifest("author.my-mod", &[]);
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, Some(fake));
    assert!(!result.is_valid());
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("does not exist"))
    );
}

/// A mod_root that exists as a directory but lacks mod.toml is a hard error.
#[test]
fn mod_root_without_manifest_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let mod_dir = tmp.path().join("author.my-mod");
    fs::create_dir_all(&mod_dir).unwrap();
    // No mod.toml written.

    let m = manifest("author.my-mod", &[]);
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, Some(&mod_dir));
    assert!(!result.is_valid());
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("mod.toml not found"))
    );
}

/// A mod_root whose directory name differs from mod.id is a hard error.
#[test]
fn mod_root_dir_name_mismatch_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let mod_dir = tmp.path().join("wrong.name");
    fs::create_dir_all(&mod_dir).unwrap();
    fs::write(mod_dir.join("mod.toml"), "# stub").unwrap();

    let m = manifest("author.my-mod", &[]); // id ≠ "wrong.name"
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, Some(&mod_dir));
    assert!(!result.is_valid());
    assert!(
        result
            .errors
            .iter()
            .any(|e| { e.message.contains("wrong.name") && e.message.contains("author.my-mod") })
    );
}

// ── 5. API-version compatibility ──────────────────────────────────────────────

/// `game_version_min` higher than the running version produces a warning,
/// not an error — the mod is still allowed to activate.
#[test]
fn future_game_version_min_is_warning_not_error() {
    let mut m = manifest("author.my-mod", &[]);
    m.info.game_version_min = "99.0.0".to_owned();
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, None);
    assert!(result.is_valid(), "unexpected errors: {:?}", result.errors);
    assert!(
        result.warnings.iter().any(|w| w.message.contains("99.0.0")),
        "expected a version-compatibility warning"
    );
}

/// `game_version_min` equal to running version has no warnings.
#[test]
fn exact_game_version_match_has_no_warnings() {
    let m = manifest("author.my-mod", &[]); // game_version_min = "0.1.0"
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, None);
    assert!(result.is_valid());
    assert!(
        result.warnings.is_empty(),
        "unexpected warnings: {:?}",
        result.warnings
    );
}

/// `game_version_min` lower than running version has no warnings either.
#[test]
fn older_game_version_min_has_no_warnings() {
    let mut m = manifest("author.my-mod", &[]);
    m.info.game_version_min = "0.0.1".to_owned();
    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, None);
    assert!(result.is_valid());
    assert!(result.warnings.is_empty());
}

// ── 6. InstalledMods::validate_and_activate integration ───────────────────────

/// `validate_and_activate` activates a valid mod.
#[test]
fn validate_and_activate_activates_valid_mod() {
    let m = manifest("author.my-mod", &[]);
    let mut installed = installed_from(vec![m]);
    installed.deactivate("author.my-mod"); // start inactive

    let result = installed.validate_and_activate("author.my-mod", "0.1.0", None);
    assert!(result.is_valid(), "errors: {:?}", result.errors);
    assert!(
        installed.is_active("author.my-mod"),
        "mod should be active after validate_and_activate"
    );
}

/// `validate_and_activate` blocks a mod with a missing dependency.
#[test]
fn validate_and_activate_blocks_mod_with_missing_dep() {
    let m = manifest("author.mod", &["base.missing"]);
    let mut installed = installed_from(vec![m]);
    installed.deactivate("author.mod");

    let result = installed.validate_and_activate("author.mod", "0.1.0", None);
    assert!(!result.is_valid());
    assert!(
        !installed.is_active("author.mod"),
        "invalid mod must not be activated"
    );
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("base.missing"))
    );
}

/// `validate_and_activate` for a mod that isn't installed returns an error.
#[test]
fn validate_and_activate_unknown_mod_returns_error() {
    let mut installed = InstalledMods::default();
    let result = installed.validate_and_activate("nonexistent.mod", "0.1.0", None);
    assert!(!result.is_valid());
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("nonexistent.mod"))
    );
}

/// A future-version warning from the pipeline is preserved in the returned
/// `ValidationResult` even though the mod is still activated.
#[test]
fn validate_and_activate_preserves_warnings() {
    let mut m = manifest("author.future-mod", &[]);
    m.info.game_version_min = "99.0.0".to_owned();
    let mut installed = installed_from(vec![m]);
    installed.deactivate("author.future-mod");

    let result = installed.validate_and_activate("author.future-mod", "0.1.0", None);
    assert!(result.is_valid(), "errors: {:?}", result.errors);
    assert!(result.warnings.iter().any(|w| w.message.contains("99.0.0")));
    assert!(
        installed.is_active("author.future-mod"),
        "valid mod with warning should still activate"
    );
}

// ── 7. Full filesystem pipeline ────────────────────────────────────────────────

/// End-to-end: write mods to disk, discover via `load_mods_from_dir`, then
/// validate and activate each one.
#[test]
fn full_pipeline_discovers_and_validates_valid_mods() {
    let tmp = TempDir::new().unwrap();
    write_mod(&tmp, "base.core", &[]);
    write_mod(&tmp, "author.ext", &["base.core"]);

    let mods = load_mods_from_dir(tmp.path());
    assert_eq!(mods.len(), 2, "expected 2 valid mods, got {}", mods.len());

    let mut installed = InstalledMods::default();
    installed.load(mods);
    installed.deactivate("base.core");
    installed.deactivate("author.ext");

    // Activate with filesystem checks.
    let base_dir = tmp.path().join("base.core");
    let r1 = installed.validate_and_activate("base.core", "0.1.0", Some(&base_dir));
    assert!(r1.is_valid(), "base.core errors: {:?}", r1.errors);
    assert!(installed.is_active("base.core"));

    let ext_dir = tmp.path().join("author.ext");
    let r2 = installed.validate_and_activate("author.ext", "0.1.0", Some(&ext_dir));
    assert!(r2.is_valid(), "author.ext errors: {:?}", r2.errors);
    assert!(installed.is_active("author.ext"));
}

/// A malformed `mod.toml` on disk is skipped by `load_mods_from_dir` and never
/// surfaces in `InstalledMods`, so the validator never sees it.
#[test]
fn malformed_manifest_on_disk_is_skipped_by_loader() {
    let tmp = TempDir::new().unwrap();
    write_mod(&tmp, "good.mod", &[]);

    // Write a broken mod.toml for a second mod.
    let bad_dir = tmp.path().join("bad.mod");
    fs::create_dir_all(&bad_dir).unwrap();
    fs::write(bad_dir.join("mod.toml"), "THIS IS NOT TOML ===").unwrap();

    let mods = load_mods_from_dir(tmp.path());
    assert_eq!(mods.len(), 1, "malformed mod should have been skipped");
    assert_eq!(mods[0].info.id, "good.mod");
}

/// A mod whose `game_version_min` exceeds the running version loads from disk
/// and validates with a warning but no error.
#[test]
fn future_targeting_mod_loads_from_disk_with_warning() {
    let tmp = TempDir::new().unwrap();
    write_mod_with_version(&tmp, "author.future", "99.0.0", &[]);

    let mods = load_mods_from_dir(tmp.path());
    assert_eq!(mods.len(), 1);

    let mut installed = InstalledMods::default();
    installed.load(mods);
    installed.deactivate("author.future");

    let mod_dir = tmp.path().join("author.future");
    let result = installed.validate_and_activate("author.future", "0.1.0", Some(&mod_dir));

    assert!(result.is_valid(), "errors: {:?}", result.errors);
    assert!(result.warnings.iter().any(|w| w.message.contains("99.0.0")));
    assert!(installed.is_active("author.future"));
}

/// Dependency ordering: a mod with an unsatisfied dep found on disk is skipped
/// at load time (loader behaviour), so it never reaches validation.
#[test]
fn loader_skips_mod_with_missing_dep_before_validation() {
    let tmp = TempDir::new().unwrap();
    // Write only the dependent; leave the dep absent.
    write_mod(&tmp, "author.dependent", &["author.absent-base"]);

    let mods = load_mods_from_dir(tmp.path());
    // Loader skips it already.
    assert!(
        mods.is_empty(),
        "mod with missing dep should have been skipped by loader"
    );
}

/// Multiple errors accumulate: a manifest with several bad fields reports all
/// of them, not just the first.
#[test]
fn multiple_schema_errors_all_reported() {
    let mut m = manifest("author.my-mod", &[]);
    m.schema_version = 5;
    m.info.name = String::new();
    m.info.version = "bad".to_owned();

    let validator = ModValidator::new("0.1.0", &[]);
    let result = validator.validate(&m, None);
    assert!(!result.is_valid());
    assert!(
        result.errors.len() >= 3,
        "expected at least 3 errors, got {:?}",
        result.errors
    );
}
