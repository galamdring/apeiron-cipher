//! Validation pass for mod asset files.
//!
//! Each `validate_*` function accepts a file path (for error context) and the
//! raw TOML source text.  They return a [`ValidationReport`] containing
//! structured [`ValidationError`]s and [`ValidationWarning`]s.
//!
//! ## Design
//!
//! - **Critical errors** (`ValidationError`) mean the asset cannot be used
//!   safely.  Callers should refuse to load the file and log `error!()`.
//! - **Warnings** (`ValidationWarning`) describe suspicious-but-loadable
//!   situations (overlapping ranges, unusually large multipliers, etc.).
//!   Callers should log `warn!()` but continue loading.
//!
//! All functions are pure — no Bevy [`App`] or file I/O.  They accept the
//! raw TOML string so they can be called from unit tests without a running
//! game.
//!
//! [`App`]: bevy::app::App

use std::collections::HashSet;

// ── Error and warning types ───────────────────────────────────────────────

/// A critical validation error in a mod asset file.
///
/// The file should not be loaded when any error is present.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    /// Path to the asset file that produced this error (for logging context).
    pub file: String,
    /// Human-readable description of what went wrong.
    pub message: String,
}

impl ValidationError {
    fn new(file: &str, message: impl Into<String>) -> Self {
        Self {
            file: file.to_owned(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[ERROR] {}: {}", self.file, self.message)
    }
}

/// A non-fatal validation warning in a mod asset file.
///
/// The file is still loaded when warnings are present, but the author should
/// review the flagged condition.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationWarning {
    /// Path to the asset file that produced this warning.
    pub file: String,
    /// Human-readable description of the suspicious condition.
    pub message: String,
}

impl ValidationWarning {
    fn new(file: &str, message: impl Into<String>) -> Self {
        Self {
            file: file.to_owned(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ValidationWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[WARN]  {}: {}", self.file, self.message)
    }
}

/// Combined result of one validation pass over a single asset file.
#[derive(Debug, Default)]
pub struct ValidationReport {
    /// Critical errors — file must not be loaded.
    pub errors: Vec<ValidationError>,
    /// Non-fatal warnings — loadable, but author should review.
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationReport {
    /// Returns `true` when there are no critical errors.
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    fn error(&mut self, file: &str, msg: impl Into<String>) {
        self.errors.push(ValidationError::new(file, msg));
    }

    fn warn(&mut self, file: &str, msg: impl Into<String>) {
        self.warnings.push(ValidationWarning::new(file, msg));
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────

/// Extract `schema_version` from a raw TOML value.
///
/// Returns `Err` if the key is absent or not an integer.
fn read_schema_version(
    table: &toml::Value,
    file: &str,
    report: &mut ValidationReport,
) -> Option<u64> {
    match table.get("schema_version") {
        None => {
            report.error(
                file,
                "missing required field 'schema_version' (must be the first field)",
            );
            None
        }
        Some(v) => match v.as_integer() {
            None => {
                report.error(
                    file,
                    format!(
                        "field 'schema_version' must be an integer, got: {:?}",
                        v.type_str()
                    ),
                );
                None
            }
            Some(n) if n != 1 => {
                report.error(
                    file,
                    format!("unsupported schema_version {n}; only version 1 is supported"),
                );
                None
            }
            Some(_) => Some(1),
        },
    }
}

/// Check that a float field exists and is within `[lo, hi]`.
///
/// Returns the value when present and in range.
fn check_float_range(
    table: &toml::Value,
    field: &str,
    lo: f64,
    hi: f64,
    file: &str,
    report: &mut ValidationReport,
) -> Option<f64> {
    match table.get(field) {
        None => {
            report.error(file, format!("missing required field '{field}'"));
            None
        }
        Some(v) => match v.as_float().or_else(|| v.as_integer().map(|i| i as f64)) {
            None => {
                report.error(
                    file,
                    format!("field '{field}' must be a number, got: {:?}", v.type_str()),
                );
                None
            }
            Some(n) if !(lo..=hi).contains(&n) => {
                report.error(
                    file,
                    format!("field '{field}' = {n} is outside valid range [{lo}, {hi}]"),
                );
                Some(n)
            }
            Some(n) => Some(n),
        },
    }
}

/// Require a string field; return `None` and record an error if absent/wrong type.
fn require_str_field<'a>(
    table: &'a toml::Value,
    field: &str,
    file: &str,
    report: &mut ValidationReport,
) -> Option<&'a str> {
    match table.get(field) {
        None => {
            report.error(file, format!("missing required field '{field}'"));
            None
        }
        Some(v) => match v.as_str() {
            None => {
                report.error(
                    file,
                    format!("field '{field}' must be a string, got: {:?}", v.type_str()),
                );
                None
            }
            s => s,
        },
    }
}

/// Validate a `PropertyRange` table (`{min, max}`) in `[0.0, 1.0]`.
///
/// Warns if `min >= max`.
fn validate_property_range(
    range_tbl: &toml::Value,
    label: &str,
    file: &str,
    report: &mut ValidationReport,
) {
    let min = check_float_range(range_tbl, "min", 0.0, 1.0, file, report);
    let max = check_float_range(range_tbl, "max", 0.0, 1.0, file, report);
    if let (Some(mn), Some(mx)) = (min, max) {
        if mn >= mx {
            report.warn(
                file,
                format!(
                    "property range '{label}' has min ({mn}) >= max ({mx}); \
                     the range will never match any material"
                ),
            );
        }
    }
}

// ── Classification file ────────────────────────────────────────────────────

/// Validate `assets/materials/classifications.toml` content.
///
/// Checks:
/// - `schema_version = 1` present
/// - Each `[[classification]]` entry has `name` and `display_name`
/// - All `PropertyRange` values are in `[0.0, 1.0]` with min < max
/// - No duplicate `name` values
/// - Warns when density or thermal_resistance ranges overlap between entries
pub fn validate_classification_file(file: &str, content: &str) -> ValidationReport {
    let mut report = ValidationReport::default();

    let value: toml::Value = match toml::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            report.error(file, format!("TOML parse error: {e}"));
            return report;
        }
    };

    read_schema_version(&value, file, &mut report);

    let entries = match value.get("classification").and_then(|v| v.as_array()) {
        None => {
            // No classifications is valid — a mod might only add combinations.
            return report;
        }
        Some(arr) => arr,
    };

    let mut seen_names: HashSet<String> = HashSet::new();

    // For overlap detection we track the density and thermal bands per entry name.
    let mut density_bands: Vec<(f64, f64, String)> = Vec::new();
    let mut thermal_bands: Vec<(f64, f64, String)> = Vec::new();

    for (idx, entry) in entries.iter().enumerate() {
        let label = format!("classification[{idx}]");

        // Required string fields.
        let name = require_str_field(entry, "name", file, &mut report)
            .unwrap_or("")
            .to_owned();
        require_str_field(entry, "display_name", file, &mut report);

        if name.is_empty() {
            // name was already reported missing; skip further checks for this entry.
            continue;
        }

        if !seen_names.insert(name.clone()) {
            report.error(
                file,
                format!("duplicate classification name '{name}' in {label}"),
            );
        }

        // Validate each optional PropertyRange sub-table.
        for prop in [
            "density",
            "thermal_resistance",
            "reactivity",
            "conductivity",
            "toxicity",
        ] {
            if let Some(range_tbl) = entry.get(prop) {
                validate_property_range(range_tbl, &format!("{label}.{prop}"), file, &mut report);

                // Collect density/thermal for overlap analysis.
                let mn = range_tbl
                    .get("min")
                    .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)));
                let mx = range_tbl
                    .get("max")
                    .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)));
                if let (Some(mn), Some(mx)) = (mn, mx) {
                    match prop {
                        "density" => density_bands.push((mn, mx, name.clone())),
                        "thermal_resistance" => thermal_bands.push((mn, mx, name.clone())),
                        _ => {}
                    }
                }
            }
        }
    }

    // Pairwise density-range overlap check.
    warn_overlapping_bands(file, "density", &density_bands, &mut report);
    warn_overlapping_bands(file, "thermal_resistance", &thermal_bands, &mut report);

    report
}

/// Emit a warning for every overlapping pair in `bands`.
fn warn_overlapping_bands(
    file: &str,
    prop: &str,
    bands: &[(f64, f64, String)],
    report: &mut ValidationReport,
) {
    for i in 0..bands.len() {
        for j in (i + 1)..bands.len() {
            let (lo_a, hi_a, ref name_a) = bands[i];
            let (lo_b, hi_b, ref name_b) = bands[j];
            // Ranges overlap when lo_a < hi_b AND lo_b < hi_a.
            if lo_a < hi_b && lo_b < hi_a {
                report.warn(
                    file,
                    format!(
                        "{prop} ranges for '{name_a}' [{lo_a}, {hi_a}] and '{name_b}' \
                         [{lo_b}, {hi_b}] overlap — first-match classification ordering \
                         may be non-deterministic"
                    ),
                );
            }
        }
    }
}

// ── Combinations file ──────────────────────────────────────────────────────

/// Validate `assets/config/combinations.toml` content.
///
/// Checks:
/// - `schema_version = 1` present
/// - Each `[[rules]]` entry has `material_seed_a` and `material_seed_b`
/// - `material_seed_a != material_seed_b` (self-combination is undefined)
/// - No duplicate (normalised) seed pairs
/// - Each present `PropertyRule` is structurally valid:
///   - `Blend`: both weights must be > 0
///   - `Catalyze`: multiplier must be > 0 (error) and ≤ 10.0 (warning)
pub fn validate_combinations_file(file: &str, content: &str) -> ValidationReport {
    let mut report = ValidationReport::default();

    let value: toml::Value = match toml::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            report.error(file, format!("TOML parse error: {e}"));
            return report;
        }
    };

    read_schema_version(&value, file, &mut report);

    let rules = match value.get("rules").and_then(|v| v.as_array()) {
        None => return report, // zero rules is valid
        Some(arr) => arr,
    };

    let mut seen_pairs: HashSet<(u64, u64)> = HashSet::new();

    for (idx, entry) in rules.iter().enumerate() {
        let label = format!("rules[{idx}]");

        let seed_a = entry
            .get("material_seed_a")
            .and_then(|v| v.as_integer())
            .map(|i| i as u64);
        let seed_b = entry
            .get("material_seed_b")
            .and_then(|v| v.as_integer())
            .map(|i| i as u64);

        if entry.get("material_seed_a").is_none() {
            report.error(
                file,
                format!("{label}: missing required field 'material_seed_a'"),
            );
        }
        if entry.get("material_seed_b").is_none() {
            report.error(
                file,
                format!("{label}: missing required field 'material_seed_b'"),
            );
        }

        if let (Some(a), Some(b)) = (seed_a, seed_b) {
            if a == b {
                report.error(
                    file,
                    format!(
                        "{label}: material_seed_a == material_seed_b ({a}); \
                             self-combination is undefined"
                    ),
                );
            } else {
                let key = if a <= b { (a, b) } else { (b, a) };
                if !seen_pairs.insert(key) {
                    report.error(
                        file,
                        format!(
                            "{label}: duplicate rule for seed pair ({}, {}); \
                             later entries override earlier ones but this is likely a mistake",
                            key.0, key.1
                        ),
                    );
                }
            }
        }

        // Validate each PropertyRule field.
        for prop in [
            "density",
            "thermal_resistance",
            "reactivity",
            "conductivity",
            "toxicity",
        ] {
            if let Some(rule) = entry.get(prop) {
                validate_property_rule(rule, &format!("{label}.{prop}"), file, &mut report);
            }
        }
    }

    report
}

/// Validate a single `PropertyRule` value (an inline TOML table).
fn validate_property_rule(
    rule: &toml::Value,
    label: &str,
    file: &str,
    report: &mut ValidationReport,
) {
    let rule_type = match rule.get("type").and_then(|v| v.as_str()) {
        None => {
            report.error(
                file,
                format!("{label}: PropertyRule missing required field 'type'"),
            );
            return;
        }
        Some(t) => t.to_owned(),
    };

    match rule_type.as_str() {
        "Blend" => {
            let wa = rule
                .get("weight_a")
                .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)));
            let wb = rule
                .get("weight_b")
                .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)));
            if rule.get("weight_a").is_none() {
                report.error(file, format!("{label}: Blend rule missing 'weight_a'"));
            } else if wa.map_or(false, |w| w <= 0.0) {
                report.error(file, format!("{label}: Blend 'weight_a' must be > 0"));
            }
            if rule.get("weight_b").is_none() {
                report.error(file, format!("{label}: Blend rule missing 'weight_b'"));
            } else if wb.map_or(false, |w| w <= 0.0) {
                report.error(file, format!("{label}: Blend 'weight_b' must be > 0"));
            }
        }
        "Catalyze" => {
            let m = rule
                .get("multiplier")
                .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)));
            if rule.get("multiplier").is_none() {
                report.error(file, format!("{label}: Catalyze rule missing 'multiplier'"));
            } else {
                match m {
                    Some(v) if v <= 0.0 => {
                        report.error(
                            file,
                            format!("{label}: Catalyze 'multiplier' = {v} must be > 0"),
                        );
                    }
                    Some(v) if v > 10.0 => {
                        report.warn(
                            file,
                            format!(
                                "{label}: Catalyze 'multiplier' = {v} is unusually large (> 10.0)"
                            ),
                        );
                    }
                    _ => {}
                }
            }
        }
        "Max" | "Min" | "Inert" => {} // no fields to validate
        other => {
            report.error(
                file,
                format!(
                    "{label}: unknown PropertyRule type '{other}'; \
                         valid types: Blend, Max, Min, Catalyze, Inert"
                ),
            );
        }
    }
}

// ── Biomes file ────────────────────────────────────────────────────────────

/// Validate `assets/config/biomes.toml` content.
///
/// Checks:
/// - `schema_version = 1` present
/// - Each `[[biomes]]` entry has `biome_type` (non-empty string)
/// - `temperature_min < temperature_max` and both in `[0.0, 1.0]`
/// - `moisture_min < moisture_max` and both in `[0.0, 1.0]`
/// - `density_modifier > 0`
/// - `ground_color` is an array of 3 floats in `[0.0, 1.0]`
/// - Each palette entry has `material_seed` and `selection_weight > 0`
pub fn validate_biomes_file(file: &str, content: &str) -> ValidationReport {
    let mut report = ValidationReport::default();

    let value: toml::Value = match toml::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            report.error(file, format!("TOML parse error: {e}"));
            return report;
        }
    };

    read_schema_version(&value, file, &mut report);

    let biomes = match value.get("biomes").and_then(|v| v.as_array()) {
        None => return report, // empty is valid
        Some(arr) => arr,
    };

    for (idx, entry) in biomes.iter().enumerate() {
        let label = format!("biomes[{idx}]");

        // biome_type must be a non-empty string.
        match require_str_field(entry, "biome_type", file, &mut report) {
            Some(s) if s.is_empty() => {
                report.error(file, format!("{label}: 'biome_type' must not be empty"));
            }
            _ => {}
        }

        // temperature_min / temperature_max
        let t_min = check_float_range(entry, "temperature_min", 0.0, 1.0, file, &mut report);
        let t_max = check_float_range(entry, "temperature_max", 0.0, 1.0, file, &mut report);
        if let (Some(mn), Some(mx)) = (t_min, t_max) {
            if mn >= mx {
                report.error(
                    file,
                    format!("{label}: temperature_min ({mn}) must be < temperature_max ({mx})"),
                );
            }
        }

        // moisture_min / moisture_max
        let m_min = check_float_range(entry, "moisture_min", 0.0, 1.0, file, &mut report);
        let m_max = check_float_range(entry, "moisture_max", 0.0, 1.0, file, &mut report);
        if let (Some(mn), Some(mx)) = (m_min, m_max) {
            if mn >= mx {
                report.error(
                    file,
                    format!("{label}: moisture_min ({mn}) must be < moisture_max ({mx})"),
                );
            }
        }

        // density_modifier must be > 0 if present.
        if let Some(dm) = entry.get("density_modifier") {
            let v = dm.as_float().or_else(|| dm.as_integer().map(|i| i as f64));
            match v {
                None => {
                    report.error(
                        file,
                        format!("{label}: 'density_modifier' must be a number"),
                    );
                }
                Some(n) if n <= 0.0 => {
                    report.error(
                        file,
                        format!("{label}: 'density_modifier' = {n} must be > 0"),
                    );
                }
                _ => {}
            }
        }

        // ground_color must be [R, G, B] each in [0.0, 1.0].
        if let Some(gc) = entry.get("ground_color") {
            validate_rgb_array(gc, &format!("{label}.ground_color"), file, &mut report);
        }

        // material_palette entries.
        if let Some(palette) = entry.get("material_palette").and_then(|v| v.as_array()) {
            for (pidx, pm) in palette.iter().enumerate() {
                let plabel = format!("{label}.material_palette[{pidx}]");
                if pm
                    .get("material_seed")
                    .and_then(|v| v.as_integer())
                    .is_none()
                {
                    report.error(
                        file,
                        format!("{plabel}: missing required field 'material_seed' (must be a u64)"),
                    );
                }
                match pm
                    .get("selection_weight")
                    .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
                {
                    None => {
                        report.error(
                            file,
                            format!("{plabel}: missing required field 'selection_weight'"),
                        );
                    }
                    Some(w) if w <= 0.0 => {
                        report.error(
                            file,
                            format!("{plabel}: 'selection_weight' = {w} must be > 0"),
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    report
}

/// Validate an RGB color array: must be 3 floats in `[0.0, 1.0]`.
fn validate_rgb_array(value: &toml::Value, label: &str, file: &str, report: &mut ValidationReport) {
    let arr = match value.as_array() {
        None => {
            report.error(
                file,
                format!("{label}: must be an array of 3 floats [R, G, B]"),
            );
            return;
        }
        Some(a) => a,
    };

    if arr.len() != 3 {
        report.error(
            file,
            format!(
                "{label}: must have exactly 3 components (R, G, B), got {}",
                arr.len()
            ),
        );
        return;
    }

    for (i, component) in arr.iter().enumerate() {
        let channel = ["R", "G", "B"][i];
        match component
            .as_float()
            .or_else(|| component.as_integer().map(|n| n as f64))
        {
            None => {
                report.error(
                    file,
                    format!("{label}[{i}] ({channel}): must be a float in [0.0, 1.0]"),
                );
            }
            Some(v) if !(0.0..=1.0).contains(&v) => {
                report.error(
                    file,
                    format!("{label}[{i}] ({channel}) = {v}: must be in [0.0, 1.0]"),
                );
            }
            _ => {}
        }
    }
}

// ── Language file ──────────────────────────────────────────────────────────

/// Validate `assets/languages/<name>.toml` content.
///
/// Checks:
/// - `schema_version = 1` present
/// - `[language]` table present
/// - Required string fields: `id`, `display_name`, `race_id`, `modality`
/// - `strangeness` in `[0.0, 1.0]`
/// - `acquisition_rate > 0`
pub fn validate_language_file(file: &str, content: &str) -> ValidationReport {
    let mut report = ValidationReport::default();

    let value: toml::Value = match toml::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            report.error(file, format!("TOML parse error: {e}"));
            return report;
        }
    };

    read_schema_version(&value, file, &mut report);

    let lang = match value.get("language") {
        None => {
            report.error(file, "missing required [language] table");
            return report;
        }
        Some(v) if !v.is_table() => {
            report.error(file, "'language' must be a TOML table");
            return report;
        }
        Some(v) => v,
    };

    require_str_field(lang, "id", file, &mut report);
    require_str_field(lang, "display_name", file, &mut report);
    require_str_field(lang, "race_id", file, &mut report);
    require_str_field(lang, "modality", file, &mut report);

    check_float_range(lang, "strangeness", 0.0, 1.0, file, &mut report);

    // acquisition_rate must be > 0.
    match lang
        .get("acquisition_rate")
        .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
    {
        None => {
            report.error(file, "missing required field '[language].acquisition_rate'");
        }
        Some(r) if r <= 0.0 => {
            report.error(
                file,
                format!("[language].acquisition_rate = {r} must be > 0"),
            );
        }
        _ => {}
    }

    report
}

// ── Race file ─────────────────────────────────────────────────────────────

/// Validate `assets/races/<name>.toml` content.
///
/// Checks:
/// - `schema_version = 1` present
/// - `[race]` table present
/// - Required string fields: `id`, `display_name`, `language_id`
/// - `strangeness` in `[0.0, 1.0]`
pub fn validate_race_file(file: &str, content: &str) -> ValidationReport {
    let mut report = ValidationReport::default();

    let value: toml::Value = match toml::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            report.error(file, format!("TOML parse error: {e}"));
            return report;
        }
    };

    read_schema_version(&value, file, &mut report);

    let race = match value.get("race") {
        None => {
            report.error(file, "missing required [race] table");
            return report;
        }
        Some(v) if !v.is_table() => {
            report.error(file, "'race' must be a TOML table");
            return report;
        }
        Some(v) => v,
    };

    require_str_field(race, "id", file, &mut report);
    require_str_field(race, "display_name", file, &mut report);
    require_str_field(race, "language_id", file, &mut report);

    check_float_range(race, "strangeness", 0.0, 1.0, file, &mut report);

    report
}

// ── Cross-reference: language ↔ race ──────────────────────────────────────

/// Cross-validate a language file and its corresponding race file.
///
/// Emits warnings when:
/// - The language's `race_id` does not match the race's `id`.
/// - The race's `language_id` does not match the language's `id`.
///
/// This function assumes both files have already passed their individual
/// validation passes (parse errors are not re-reported here).
pub fn validate_language_race_crossref(
    lang_file: &str,
    lang_content: &str,
    race_file: &str,
    race_content: &str,
) -> ValidationReport {
    let mut report = ValidationReport::default();

    let lang_val: toml::Value = match toml::from_str(lang_content) {
        Ok(v) => v,
        Err(_) => return report, // already reported by individual pass
    };
    let race_val: toml::Value = match toml::from_str(race_content) {
        Ok(v) => v,
        Err(_) => return report,
    };

    let lang_id = lang_val
        .get("language")
        .and_then(|t| t.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let race_id_in_lang = lang_val
        .get("language")
        .and_then(|t| t.get("race_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let race_id = race_val
        .get("race")
        .and_then(|t| t.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let lang_id_in_race = race_val
        .get("race")
        .and_then(|t| t.get("language_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // language.race_id should equal race.id.
    if !race_id_in_lang.is_empty() && !race_id.is_empty() && race_id_in_lang != race_id {
        report.warn(
            lang_file,
            format!(
                "[language].race_id = '{race_id_in_lang}' does not match \
                 [race].id = '{race_id}' in '{race_file}'"
            ),
        );
    }

    // race.language_id should equal language.id.
    if !lang_id_in_race.is_empty() && !lang_id.is_empty() && lang_id_in_race != lang_id {
        report.warn(
            race_file,
            format!(
                "[race].language_id = '{lang_id_in_race}' does not match \
                 [language].id = '{lang_id}' in '{lang_file}'"
            ),
        );
    }

    report
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── classification ────────────────────────────────────────────────

    #[test]
    fn classification_valid_file_passes() {
        let toml = r#"
schema_version = 1

[[classification]]
name         = "ferrite"
display_name = "Ferrite"

[classification.density]
min = 0.45
max = 0.57
"#;
        let r = validate_classification_file("test.toml", toml);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn classification_missing_schema_version_is_error() {
        let toml = r#"
[[classification]]
name         = "ferrite"
display_name = "Ferrite"
"#;
        let r = validate_classification_file("test.toml", toml);
        assert!(
            r.errors
                .iter()
                .any(|e| e.message.contains("schema_version"))
        );
    }

    #[test]
    fn classification_wrong_schema_version_is_error() {
        let toml = r#"
schema_version = 2
[[classification]]
name         = "ferrite"
display_name = "Ferrite"
"#;
        let r = validate_classification_file("test.toml", toml);
        assert!(
            r.errors
                .iter()
                .any(|e| e.message.contains("unsupported schema_version"))
        );
    }

    #[test]
    fn classification_duplicate_name_is_error() {
        let toml = r#"
schema_version = 1
[[classification]]
name         = "ferrite"
display_name = "Ferrite"
[[classification]]
name         = "ferrite"
display_name = "Ferrite Again"
"#;
        let r = validate_classification_file("test.toml", toml);
        assert!(
            r.errors
                .iter()
                .any(|e| e.message.contains("duplicate classification name"))
        );
    }

    #[test]
    fn classification_range_min_equals_max_is_warning() {
        let toml = r#"
schema_version = 1
[[classification]]
name         = "weird"
display_name = "Weird"
[classification.density]
min = 0.5
max = 0.5
"#;
        let r = validate_classification_file("test.toml", toml);
        assert!(r.warnings.iter().any(|w| w.message.contains("never match")));
    }

    #[test]
    fn classification_range_out_of_bounds_is_error() {
        let toml = r#"
schema_version = 1
[[classification]]
name         = "overload"
display_name = "Overload"
[classification.density]
min = 0.1
max = 1.5
"#;
        let r = validate_classification_file("test.toml", toml);
        assert!(
            r.errors
                .iter()
                .any(|e| e.message.contains("outside valid range"))
        );
    }

    #[test]
    fn classification_overlapping_density_ranges_warns() {
        let toml = r#"
schema_version = 1
[[classification]]
name         = "alpha"
display_name = "Alpha"
[classification.density]
min = 0.1
max = 0.5

[[classification]]
name         = "beta"
display_name = "Beta"
[classification.density]
min = 0.4
max = 0.8
"#;
        let r = validate_classification_file("test.toml", toml);
        assert!(r.warnings.iter().any(|w| w.message.contains("overlap")));
    }

    // ── combinations ─────────────────────────────────────────────────

    #[test]
    fn combinations_valid_file_passes() {
        let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 9002
material_seed_b = 1001
density            = { type = "Catalyze", multiplier = 1.2 }
thermal_resistance = { type = "Max" }
reactivity         = { type = "Min" }
conductivity       = { type = "Max" }
toxicity           = { type = "Blend", weight_a = 0.7, weight_b = 0.3 }
"#;
        let r = validate_combinations_file("test.toml", toml);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn combinations_self_combination_is_error() {
        let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 1001
material_seed_b = 1001
"#;
        let r = validate_combinations_file("test.toml", toml);
        assert!(
            r.errors
                .iter()
                .any(|e| e.message.contains("self-combination"))
        );
    }

    #[test]
    fn combinations_duplicate_pair_is_error() {
        let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 9002
material_seed_b = 1001
[[rules]]
material_seed_a = 1001
material_seed_b = 9002
"#;
        let r = validate_combinations_file("test.toml", toml);
        assert!(
            r.errors
                .iter()
                .any(|e| e.message.contains("duplicate rule"))
        );
    }

    #[test]
    fn combinations_catalyze_zero_multiplier_is_error() {
        let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 9002
material_seed_b = 1001
density = { type = "Catalyze", multiplier = 0.0 }
"#;
        let r = validate_combinations_file("test.toml", toml);
        assert!(r.errors.iter().any(|e| e.message.contains("multiplier")));
    }

    #[test]
    fn combinations_catalyze_large_multiplier_warns() {
        let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 9002
material_seed_b = 1001
density = { type = "Catalyze", multiplier = 15.0 }
"#;
        let r = validate_combinations_file("test.toml", toml);
        assert!(
            r.warnings
                .iter()
                .any(|w| w.message.contains("unusually large"))
        );
    }

    #[test]
    fn combinations_blend_zero_weight_is_error() {
        let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 9002
material_seed_b = 1001
toxicity = { type = "Blend", weight_a = 0.0, weight_b = 1.0 }
"#;
        let r = validate_combinations_file("test.toml", toml);
        assert!(r.errors.iter().any(|e| e.message.contains("weight_a")));
    }

    // ── biomes ────────────────────────────────────────────────────────

    #[test]
    fn biomes_valid_file_passes() {
        let toml = r#"
schema_version = 1
[[biomes]]
biome_type      = "crystal_lichen_fields"
temperature_min = 0.0
temperature_max = 0.25
moisture_min    = 0.5
moisture_max    = 0.85
ground_color    = [0.41, 0.63, 0.72]
density_modifier = 0.85

[[biomes.material_palette]]
material_seed    = 9002
selection_weight = 4.0
"#;
        let r = validate_biomes_file("test.toml", toml);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn biomes_temperature_min_exceeds_max_is_error() {
        let toml = r#"
schema_version = 1
[[biomes]]
biome_type      = "bad_biome"
temperature_min = 0.9
temperature_max = 0.1
moisture_min    = 0.0
moisture_max    = 1.0
"#;
        let r = validate_biomes_file("test.toml", toml);
        assert!(
            r.errors
                .iter()
                .any(|e| e.message.contains("temperature_min"))
        );
    }

    #[test]
    fn biomes_moisture_min_equals_max_is_error() {
        let toml = r#"
schema_version = 1
[[biomes]]
biome_type      = "flat_biome"
temperature_min = 0.0
temperature_max = 1.0
moisture_min    = 0.5
moisture_max    = 0.5
"#;
        let r = validate_biomes_file("test.toml", toml);
        assert!(r.errors.iter().any(|e| e.message.contains("moisture_min")));
    }

    #[test]
    fn biomes_negative_density_modifier_is_error() {
        let toml = r#"
schema_version = 1
[[biomes]]
biome_type       = "neg_density"
temperature_min  = 0.0
temperature_max  = 1.0
moisture_min     = 0.0
moisture_max     = 1.0
density_modifier = -0.5
"#;
        let r = validate_biomes_file("test.toml", toml);
        assert!(
            r.errors
                .iter()
                .any(|e| e.message.contains("density_modifier"))
        );
    }

    #[test]
    fn biomes_ground_color_out_of_range_is_error() {
        let toml = r#"
schema_version = 1
[[biomes]]
biome_type      = "colorful_biome"
temperature_min = 0.0
temperature_max = 1.0
moisture_min    = 0.0
moisture_max    = 1.0
ground_color    = [0.5, 1.5, 0.5]
"#;
        let r = validate_biomes_file("test.toml", toml);
        assert!(r.errors.iter().any(|e| e.message.contains("ground_color")));
    }

    #[test]
    fn biomes_palette_zero_weight_is_error() {
        let toml = r#"
schema_version = 1
[[biomes]]
biome_type      = "light_biome"
temperature_min = 0.0
temperature_max = 1.0
moisture_min    = 0.0
moisture_max    = 1.0

[[biomes.material_palette]]
material_seed    = 1001
selection_weight = 0.0
"#;
        let r = validate_biomes_file("test.toml", toml);
        assert!(
            r.errors
                .iter()
                .any(|e| e.message.contains("selection_weight"))
        );
    }

    // ── language ──────────────────────────────────────────────────────

    #[test]
    fn language_valid_file_passes() {
        let toml = r#"
schema_version = 1
[language]
id               = "deep_sign"
display_name     = "Deep-Sign"
race_id          = "veth"
modality         = "gestural"
strangeness      = 0.82
acquisition_rate = 0.65
"#;
        let r = validate_language_file("test.toml", toml);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn language_missing_race_id_is_error() {
        let toml = r#"
schema_version = 1
[language]
id               = "deep_sign"
display_name     = "Deep-Sign"
modality         = "gestural"
strangeness      = 0.82
acquisition_rate = 0.65
"#;
        let r = validate_language_file("test.toml", toml);
        assert!(r.errors.iter().any(|e| e.message.contains("race_id")));
    }

    #[test]
    fn language_strangeness_out_of_range_is_error() {
        let toml = r#"
schema_version = 1
[language]
id               = "weird"
display_name     = "Weird"
race_id          = "aliens"
modality         = "vocal"
strangeness      = 1.5
acquisition_rate = 1.0
"#;
        let r = validate_language_file("test.toml", toml);
        assert!(r.errors.iter().any(|e| e.message.contains("strangeness")));
    }

    #[test]
    fn language_zero_acquisition_rate_is_error() {
        let toml = r#"
schema_version = 1
[language]
id               = "silent"
display_name     = "Silent"
race_id          = "ghosts"
modality         = "written"
strangeness      = 0.5
acquisition_rate = 0.0
"#;
        let r = validate_language_file("test.toml", toml);
        assert!(
            r.errors
                .iter()
                .any(|e| e.message.contains("acquisition_rate"))
        );
    }

    // ── race ──────────────────────────────────────────────────────────

    #[test]
    fn race_valid_file_passes() {
        let toml = r#"
schema_version = 1
[race]
id           = "veth"
display_name = "Veth"
language_id  = "deep_sign"
strangeness  = 0.78
"#;
        let r = validate_race_file("test.toml", toml);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn race_missing_language_id_is_error() {
        let toml = r#"
schema_version = 1
[race]
id           = "veth"
display_name = "Veth"
strangeness  = 0.78
"#;
        let r = validate_race_file("test.toml", toml);
        assert!(r.errors.iter().any(|e| e.message.contains("language_id")));
    }

    #[test]
    fn race_strangeness_below_zero_is_error() {
        let toml = r#"
schema_version = 1
[race]
id           = "friendly"
display_name = "Very Friendly"
language_id  = "common"
strangeness  = -0.1
"#;
        let r = validate_race_file("test.toml", toml);
        assert!(r.errors.iter().any(|e| e.message.contains("strangeness")));
    }

    // ── crossref ─────────────────────────────────────────────────────

    #[test]
    fn crossref_matching_ids_no_warning() {
        let lang = r#"
schema_version = 1
[language]
id               = "deep_sign"
display_name     = "Deep-Sign"
race_id          = "veth"
modality         = "gestural"
strangeness      = 0.82
acquisition_rate = 0.65
"#;
        let race = r#"
schema_version = 1
[race]
id           = "veth"
display_name = "Veth"
language_id  = "deep_sign"
strangeness  = 0.78
"#;
        let r = validate_language_race_crossref("lang.toml", lang, "race.toml", race);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn crossref_mismatched_race_id_warns() {
        let lang = r#"
schema_version = 1
[language]
id               = "deep_sign"
display_name     = "Deep-Sign"
race_id          = "wrong_race"
modality         = "gestural"
strangeness      = 0.82
acquisition_rate = 0.65
"#;
        let race = r#"
schema_version = 1
[race]
id           = "veth"
display_name = "Veth"
language_id  = "deep_sign"
strangeness  = 0.78
"#;
        let r = validate_language_race_crossref("lang.toml", lang, "race.toml", race);
        assert!(r.warnings.iter().any(|w| w.message.contains("race_id")));
    }

    #[test]
    fn crossref_mismatched_language_id_warns() {
        let lang = r#"
schema_version = 1
[language]
id               = "deep_sign"
display_name     = "Deep-Sign"
race_id          = "veth"
modality         = "gestural"
strangeness      = 0.82
acquisition_rate = 0.65
"#;
        let race = r#"
schema_version = 1
[race]
id           = "veth"
display_name = "Veth"
language_id  = "wrong_lang"
strangeness  = 0.78
"#;
        let r = validate_language_race_crossref("lang.toml", lang, "race.toml", race);
        assert!(r.warnings.iter().any(|w| w.message.contains("language_id")));
    }
}
