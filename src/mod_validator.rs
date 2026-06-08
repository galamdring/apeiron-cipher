//! Mod validation pipeline for the Apeiron Cipher mod system (Epic 23).
//!
//! [`ModValidator`] runs all integrity checks against a [`ModManifest`]
//! *before* [`InstalledMods::activate`] allows the mod to become active.
//! This module is intentionally pure: no Bevy resources, no global state,
//! no side-effects beyond the returned [`ValidationResult`].
//!
//! # Checks performed
//!
//! 1. **Schema validation** — `schema_version` must be `1`; required fields
//!    (`id`, `name`, `version`, `game_version_min`, `spdx_license`) must be
//!    non-empty; the `id` must be in `author.slug` dot-notation form.
//!
//! 2. **Version parsing** — `mod.version` and `mod.game_version_min` must each
//!    parse as `MAJOR.MINOR.PATCH` semver triples.
//!
//! 3. **API / host-version compatibility** — if `game_version_min` is higher
//!    than the running game version the validator adds a [`ValidationWarning`]
//!    (load continues but the operator is notified).
//!
//! 4. **Entry-point existence** — the mod's root directory must exist on disk.
//!    If the caller supplies a `mod_root` path the validator also checks that
//!    `mod.toml` is present there.
//!
//! 5. **Dependency resolution** — every declared dependency must be present
//!    in the provided set of installed mod IDs.  Missing dependencies produce
//!    an error.  Cycle detection is the responsibility of the topo-sort in
//!    [`super::mod_manifest`] (which runs before activation); the validator
//!    checks only "all deps are installed", not "no cycle".
//!
//! # Usage
//!
//! ```rust,ignore
//! use apeiron_cipher::mod_validator::ModValidator;
//!
//! let validator = ModValidator::new(
//!     "0.1.0",               // running game version
//!     &installed_mod_ids,    // &[&str] of all currently-installed mod IDs
//! );
//! let result = validator.validate(&manifest, Some(&mod_root_path));
//! if result.is_valid() {
//!     installed.activate(&manifest.info.id);
//! }
//! ```

use std::{fmt, path::Path};

use crate::mod_manifest::ModManifest;

// ── Public result types ───────────────────────────────────────────────────────

/// A single hard error that prevents a mod from being activated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    /// Human-readable description of what is wrong.
    pub message: String,
}

impl ValidationError {
    /// Creates a new `ValidationError` with the given message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error: {}", self.message)
    }
}

/// A non-fatal warning.  The mod can still be activated; the warning is
/// surfaced to the operator (e.g. via `warn!`) but does not block activation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationWarning {
    /// Human-readable description of the issue.
    pub message: String,
}

impl ValidationWarning {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ValidationWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "warning: {}", self.message)
    }
}

/// The complete result of running the validation pipeline against one mod.
///
/// A result is considered **valid** (eligible for activation) when
/// [`errors`][Self::errors] is empty.  Warnings are informational only.
#[derive(Clone, Debug, Default)]
pub struct ValidationResult {
    /// Hard errors — any non-empty list means the mod must not be activated.
    pub errors: Vec<ValidationError>,

    /// Non-fatal warnings — the mod may be activated but the operator should
    /// be notified.
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    /// Returns `true` when there are no errors (warnings are allowed).
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    fn push_error(&mut self, message: impl Into<String>) {
        self.errors.push(ValidationError::new(message));
    }

    fn push_warning(&mut self, message: impl Into<String>) {
        self.warnings.push(ValidationWarning::new(message));
    }
}

// ── Validator ─────────────────────────────────────────────────────────────────

/// Stateless validator for [`ModManifest`] instances.
///
/// Construct one validator per activation pass with the current host-version
/// and the set of installed mod IDs, then call [`validate`][Self::validate]
/// once per manifest.
pub struct ModValidator<'a> {
    /// Running game version string (e.g. `"0.1.0"` from `Cargo.toml`).
    game_version: &'a str,

    /// All currently-installed mod IDs (used for dependency resolution).
    installed_ids: &'a [&'a str],
}

impl<'a> ModValidator<'a> {
    /// Creates a new `ModValidator`.
    ///
    /// # Parameters
    ///
    /// - `game_version`: the running game version, e.g. `"0.1.0"`.
    /// - `installed_ids`: slice of all mod IDs currently installed (i.e. in
    ///   `InstalledMods::all_mods`).  Used to check that every declared
    ///   dependency is satisfied.
    pub fn new(game_version: &'a str, installed_ids: &'a [&'a str]) -> Self {
        Self {
            game_version,
            installed_ids,
        }
    }

    /// Runs the full validation pipeline against `manifest`.
    ///
    /// - `mod_root`: optional filesystem path to the mod's root directory.
    ///   When supplied the validator checks that the directory and `mod.toml`
    ///   exist on disk (entry-point existence check).  When `None` the
    ///   filesystem checks are skipped — useful in unit tests that work
    ///   entirely with in-memory manifests.
    pub fn validate(&self, manifest: &ModManifest, mod_root: Option<&Path>) -> ValidationResult {
        let mut result = ValidationResult::default();

        self.check_schema_version(manifest, &mut result);
        self.check_required_fields(manifest, &mut result);
        self.check_id_format(manifest, &mut result);
        self.check_version_format(manifest, &mut result);
        self.check_game_version_compatibility(manifest, &mut result);
        self.check_entry_point(manifest, mod_root, &mut result);
        self.check_dependencies(manifest, &mut result);

        result
    }

    // ── Individual checks ─────────────────────────────────────────────────────

    /// (1) schema_version must be the value `1`.
    fn check_schema_version(&self, manifest: &ModManifest, result: &mut ValidationResult) {
        if manifest.schema_version != 1 {
            result.push_error(format!(
                "unsupported schema_version {} — only schema_version 1 is understood",
                manifest.schema_version
            ));
        }
    }

    /// (2) Required fields must be non-empty strings.
    fn check_required_fields(&self, manifest: &ModManifest, result: &mut ValidationResult) {
        let required: &[(&str, &str)] = &[
            ("mod.id", &manifest.info.id),
            ("mod.name", &manifest.info.name),
            ("mod.version", &manifest.info.version),
            ("mod.game_version_min", &manifest.info.game_version_min),
            ("licensing.spdx_license", &manifest.licensing.spdx_license),
        ];

        for (field, value) in required {
            if value.trim().is_empty() {
                result.push_error(format!("required field `{field}` is empty"));
            }
        }
    }

    /// (3) `mod.id` must be in `author.slug` dot-notation: at least one dot,
    ///     only alphanumerics, hyphens, and dots.
    fn check_id_format(&self, manifest: &ModManifest, result: &mut ValidationResult) {
        let id = &manifest.info.id;
        if id.is_empty() {
            // already caught by check_required_fields
            return;
        }

        let valid_chars = id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_');
        let has_dot = id.contains('.');
        let no_leading_trailing_dot = !id.starts_with('.') && !id.ends_with('.');
        let no_double_dot = !id.contains("..");

        if !valid_chars || !has_dot || !no_leading_trailing_dot || !no_double_dot {
            result.push_error(format!(
                "mod.id `{id}` is not in `author.slug` dot-notation \
                 (allowed: alphanumerics, hyphens, underscores, dots; \
                 must contain at least one dot; must not start/end with a dot)"
            ));
        }
    }

    /// (4) `mod.version` and `mod.game_version_min` must parse as `X.Y.Z`.
    fn check_version_format(&self, manifest: &ModManifest, result: &mut ValidationResult) {
        for (field, value) in [
            ("mod.version", &manifest.info.version),
            ("mod.game_version_min", &manifest.info.game_version_min),
        ] {
            if value.is_empty() {
                // already caught by check_required_fields
                continue;
            }
            if parse_semver(value).is_none() {
                result.push_error(format!(
                    "field `{field}` value `{value}` is not a valid MAJOR.MINOR.PATCH \
                     semver triple"
                ));
            }
        }
    }

    /// (5) API / host-version compatibility: warn if `game_version_min > game_version`.
    fn check_game_version_compatibility(
        &self,
        manifest: &ModManifest,
        result: &mut ValidationResult,
    ) {
        let min = &manifest.info.game_version_min;
        let running = self.game_version;

        // Skip if either version is unparseable (already flagged by format check).
        if let (Some(min_v), Some(run_v)) = (parse_semver(min), parse_semver(running))
            && min_v > run_v
        {
            result.push_warning(format!(
                "mod targets game_version_min `{min}` but the running \
                 game version is `{running}` — the mod may not work correctly"
            ));
        }
    }

    /// (6) Entry-point existence: if a mod_root path is supplied, check that
    ///     the directory and its `mod.toml` exist on disk.
    fn check_entry_point(
        &self,
        manifest: &ModManifest,
        mod_root: Option<&Path>,
        result: &mut ValidationResult,
    ) {
        let Some(root) = mod_root else { return };

        if !root.exists() || !root.is_dir() {
            result.push_error(format!(
                "mod root directory `{}` does not exist on disk",
                root.display()
            ));
            return;
        }

        let manifest_path = root.join("mod.toml");
        if !manifest_path.exists() {
            result.push_error(format!(
                "mod.toml not found at `{}` — directory exists but has no manifest",
                manifest_path.display()
            ));
        }

        // Verify the directory name matches mod.id (structural requirement
        // from the loader; re-checked here so the validator is self-contained).
        if let Some(dir_name) = root.file_name().and_then(|n| n.to_str()) {
            if dir_name != manifest.info.id {
                result.push_error(format!(
                    "mod directory name `{dir_name}` does not match mod.id `{}` — \
                     directory name and mod.id must be identical",
                    manifest.info.id
                ));
            }
        }
    }

    /// (7) Dependency resolution: every declared dependency must be in
    ///     `installed_ids`.
    fn check_dependencies(&self, manifest: &ModManifest, result: &mut ValidationResult) {
        for dep in &manifest.info.dependencies {
            if !self.installed_ids.contains(&dep.as_str()) {
                result.push_error(format!(
                    "declared dependency `{dep}` is not in the installed mod set"
                ));
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parses a `"MAJOR.MINOR.PATCH"` version string into a comparable `(u64, u64, u64)`.
///
/// Returns `None` if the string does not match that exact three-part form.
fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let major = parts[0].parse::<u64>().ok()?;
    let minor = parts[1].parse::<u64>().ok()?;
    let patch = parts[2].parse::<u64>().ok()?;
    Some((major, minor, patch))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_manifest::{ModInfo, ModLicensing, ModManifest};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn valid_manifest(id: &str) -> ModManifest {
        valid_manifest_with_deps(id, &[])
    }

    fn valid_manifest_with_deps(id: &str, deps: &[&str]) -> ModManifest {
        ModManifest {
            schema_version: 1,
            info: ModInfo {
                id: id.to_owned(),
                name: "Test Mod".to_owned(),
                version: "0.1.0".to_owned(),
                description: "A test mod.".to_owned(),
                author: "Tester".to_owned(),
                dependencies: deps.iter().map(|s| s.to_string()).collect(),
                game_version_min: "0.1.0".to_owned(),
            },
            licensing: ModLicensing {
                spdx_license: "CC-BY-4.0".to_owned(),
                free_distribution_url: String::new(),
            },
        }
    }

    fn validator_no_installed() -> ModValidator<'static> {
        ModValidator::new("0.1.0", &[])
    }

    // ── parse_semver ──────────────────────────────────────────────────────────

    #[test]
    fn semver_valid() {
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("0.0.0"), Some((0, 0, 0)));
        assert_eq!(parse_semver("10.20.30"), Some((10, 20, 30)));
    }

    #[test]
    fn semver_invalid() {
        assert!(parse_semver("1.2").is_none());
        assert!(parse_semver("1.2.3.4").is_none());
        assert!(parse_semver("abc").is_none());
        assert!(parse_semver("").is_none());
        assert!(parse_semver("1.2.x").is_none());
    }

    // ── Schema version ────────────────────────────────────────────────────────

    #[test]
    fn valid_manifest_passes() {
        let m = valid_manifest("author.my-mod");
        let result = validator_no_installed().validate(&m, None);
        assert!(result.is_valid(), "errors: {:?}", result.errors);
        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
    }

    #[test]
    fn wrong_schema_version_is_error() {
        let mut m = valid_manifest("author.my-mod");
        m.schema_version = 2;
        let result = validator_no_installed().validate(&m, None);
        assert!(!result.is_valid());
        assert!(result.errors[0].message.contains("schema_version 2"));
    }

    // ── Required fields ───────────────────────────────────────────────────────

    #[test]
    fn empty_id_is_error() {
        let mut m = valid_manifest("author.my-mod");
        m.info.id = String::new();
        let result = validator_no_installed().validate(&m, None);
        assert!(!result.is_valid());
        let msgs: Vec<&str> = result.errors.iter().map(|e| e.message.as_str()).collect();
        assert!(msgs.iter().any(|m| m.contains("mod.id")));
    }

    #[test]
    fn empty_name_is_error() {
        let mut m = valid_manifest("author.my-mod");
        m.info.name = String::new();
        let result = validator_no_installed().validate(&m, None);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.message.contains("mod.name")));
    }

    #[test]
    fn empty_spdx_license_is_error() {
        let mut m = valid_manifest("author.my-mod");
        m.licensing.spdx_license = String::new();
        let result = validator_no_installed().validate(&m, None);
        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("spdx_license"))
        );
    }

    // ── ID format ─────────────────────────────────────────────────────────────

    #[test]
    fn id_without_dot_is_error() {
        let mut m = valid_manifest("nodotmod");
        m.info.id = "nodotmod".to_owned();
        let result = validator_no_installed().validate(&m, None);
        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("dot-notation"))
        );
    }

    #[test]
    fn id_with_leading_dot_is_error() {
        let mut m = valid_manifest(".author.mod");
        m.info.id = ".author.mod".to_owned();
        let result = validator_no_installed().validate(&m, None);
        assert!(!result.is_valid());
    }

    #[test]
    fn id_with_hyphen_and_dot_is_valid() {
        let m = valid_manifest("my-author.my-mod");
        let result = validator_no_installed().validate(&m, None);
        assert!(result.is_valid(), "errors: {:?}", result.errors);
    }

    // ── Version format ────────────────────────────────────────────────────────

    #[test]
    fn bad_version_string_is_error() {
        let mut m = valid_manifest("author.my-mod");
        m.info.version = "1.0".to_owned(); // only two parts
        let result = validator_no_installed().validate(&m, None);
        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("mod.version"))
        );
    }

    #[test]
    fn bad_game_version_min_is_error() {
        let mut m = valid_manifest("author.my-mod");
        m.info.game_version_min = "not-semver".to_owned();
        let result = validator_no_installed().validate(&m, None);
        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("game_version_min"))
        );
    }

    // ── Game-version compatibility ────────────────────────────────────────────

    #[test]
    fn mod_targeting_future_game_version_is_warning_not_error() {
        let mut m = valid_manifest("author.my-mod");
        m.info.game_version_min = "99.0.0".to_owned();
        let validator = ModValidator::new("0.1.0", &[]);
        let result = validator.validate(&m, None);
        // Must be no hard errors from the compatibility check itself
        // (version format is valid "99.0.0", so no format error either).
        assert!(result.is_valid(), "errors: {:?}", result.errors);
        assert!(
            result.warnings.iter().any(|w| w.message.contains("99.0.0")),
            "expected version-compatibility warning"
        );
    }

    #[test]
    fn mod_targeting_same_game_version_has_no_warning() {
        let m = valid_manifest("author.my-mod"); // game_version_min = "0.1.0"
        let validator = ModValidator::new("0.1.0", &[]);
        let result = validator.validate(&m, None);
        assert!(result.is_valid());
        assert!(result.warnings.is_empty());
    }

    // ── Entry-point existence ─────────────────────────────────────────────────

    #[test]
    fn missing_mod_root_dir_is_error() {
        let m = valid_manifest("author.my-mod");
        let fake_path = std::path::Path::new("/tmp/definitely_does_not_exist_abc123/author.my-mod");
        let result = validator_no_installed().validate(&m, Some(fake_path));
        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("does not exist"))
        );
    }

    #[test]
    fn dir_exists_but_no_mod_toml_is_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mod_dir = tmp.path().join("author.my-mod");
        std::fs::create_dir_all(&mod_dir).unwrap();
        // Note: deliberately do NOT write mod.toml

        let m = valid_manifest("author.my-mod");
        let result = validator_no_installed().validate(&m, Some(&mod_dir));
        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("mod.toml not found"))
        );
    }

    #[test]
    fn valid_mod_dir_with_mod_toml_passes_entry_point_check() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mod_dir = tmp.path().join("author.my-mod");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::write(mod_dir.join("mod.toml"), "# stub").unwrap();

        let m = valid_manifest("author.my-mod");
        let result = validator_no_installed().validate(&m, Some(&mod_dir));
        assert!(result.is_valid(), "errors: {:?}", result.errors);
    }

    #[test]
    fn mod_dir_name_mismatch_is_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mod_dir = tmp.path().join("wrong.dir-name");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::write(mod_dir.join("mod.toml"), "# stub").unwrap();

        let m = valid_manifest("author.my-mod"); // id ≠ dir name
        let result = validator_no_installed().validate(&m, Some(&mod_dir));
        assert!(!result.is_valid());
        assert!(
            result.errors.iter().any(
                |e| e.message.contains("wrong.dir-name") && e.message.contains("author.my-mod")
            )
        );
    }

    // ── Dependency resolution ─────────────────────────────────────────────────

    #[test]
    fn satisfied_dependency_passes() {
        let m = valid_manifest_with_deps("author.mod", &["base.core"]);
        let validator = ModValidator::new("0.1.0", &["base.core"]);
        let result = validator.validate(&m, None);
        assert!(result.is_valid(), "errors: {:?}", result.errors);
    }

    #[test]
    fn missing_dependency_is_error() {
        let m = valid_manifest_with_deps("author.mod", &["base.core"]);
        let validator = ModValidator::new("0.1.0", &[]); // base.core not installed
        let result = validator.validate(&m, None);
        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("base.core"))
        );
    }

    #[test]
    fn multiple_missing_deps_each_produce_an_error() {
        let m = valid_manifest_with_deps("author.mod", &["a.dep", "b.dep", "c.dep"]);
        let validator = ModValidator::new("0.1.0", &["a.dep"]); // b.dep, c.dep missing
        let result = validator.validate(&m, None);
        assert!(!result.is_valid());
        assert_eq!(result.errors.len(), 2);
        let msgs: String = result.errors.iter().map(|e| e.message.clone()).collect();
        assert!(msgs.contains("b.dep"));
        assert!(msgs.contains("c.dep"));
    }

    #[test]
    fn no_dependencies_always_passes_dep_check() {
        let m = valid_manifest("author.standalone");
        let validator = ModValidator::new("0.1.0", &[]);
        let result = validator.validate(&m, None);
        assert!(result.is_valid(), "errors: {:?}", result.errors);
    }

    // ── Combined / edge cases ─────────────────────────────────────────────────

    #[test]
    fn multiple_errors_accumulate() {
        let mut m = valid_manifest("author.my-mod");
        m.schema_version = 99;
        m.info.name = String::new();
        m.info.version = "bad".to_owned();
        let result = validator_no_installed().validate(&m, None);
        assert!(!result.is_valid());
        assert!(
            result.errors.len() >= 3,
            "expected at least 3 errors, got {:?}",
            result.errors
        );
    }

    #[test]
    fn is_valid_reflects_error_list() {
        let good = ValidationResult::default();
        assert!(good.is_valid());

        let mut bad = ValidationResult::default();
        bad.push_error("something broke");
        assert!(!bad.is_valid());
    }
}
