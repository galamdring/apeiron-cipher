//! Integration tests for [`apeiron_cipher::mod_asset_validator`].
//!
//! Each test feeds deliberately invalid mod-asset content through the public
//! validator functions and asserts that the expected structured error or
//! warning message is produced.
//!
//! In addition, several tests validate the real example-mod assets shipped in
//! the `mods/` directory to guard against regressions.

use apeiron_cipher::mod_asset_validator::{
    validate_biomes_file, validate_classification_file, validate_combinations_file,
    validate_language_file, validate_language_race_crossref, validate_race_file,
};

// ── helpers ────────────────────────────────────────────────────────────────

/// Asserts that at least one error message contains the given substring.
#[track_caller]
fn assert_error_contains(
    errors: &[apeiron_cipher::mod_asset_validator::ValidationError],
    needle: &str,
) {
    assert!(
        errors.iter().any(|e| e.message.contains(needle)),
        "expected an error containing {:?}, got: {:?}",
        needle,
        errors
    );
}

/// Asserts that there are no errors.
#[track_caller]
fn assert_no_errors(errors: &[apeiron_cipher::mod_asset_validator::ValidationError]) {
    assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
}

/// Asserts that at least one warning message contains the given substring.
#[track_caller]
fn assert_warning_contains(
    warnings: &[apeiron_cipher::mod_asset_validator::ValidationWarning],
    needle: &str,
) {
    assert!(
        warnings.iter().any(|w| w.message.contains(needle)),
        "expected a warning containing {:?}, got: {:?}",
        needle,
        warnings
    );
}

// ── Classification: invalid inputs ────────────────────────────────────────

#[test]
fn classification_invalid_toml_produces_parse_error() {
    let bad = "schema_version = 1\n[[classification\nbroken";
    let r = validate_classification_file("broken.toml", bad);
    assert_error_contains(&r.errors, "TOML parse error");
}

#[test]
fn classification_absent_schema_version_is_error() {
    let toml = r#"
[[classification]]
name         = "ferrite"
display_name = "Ferrite"
"#;
    let r = validate_classification_file("cls.toml", toml);
    assert_error_contains(&r.errors, "schema_version");
}

#[test]
fn classification_wrong_schema_version_rejected() {
    let toml = r#"
schema_version = 99
[[classification]]
name         = "ferrite"
display_name = "Ferrite"
"#;
    let r = validate_classification_file("cls.toml", toml);
    assert_error_contains(&r.errors, "unsupported schema_version");
}

#[test]
fn classification_missing_name_field_is_error() {
    let toml = r#"
schema_version = 1
[[classification]]
display_name = "No Name"
[classification.density]
min = 0.1
max = 0.5
"#;
    let r = validate_classification_file("cls.toml", toml);
    assert_error_contains(&r.errors, "name");
}

#[test]
fn classification_missing_display_name_is_error() {
    let toml = r#"
schema_version = 1
[[classification]]
name = "no_display"
[classification.density]
min = 0.1
max = 0.5
"#;
    let r = validate_classification_file("cls.toml", toml);
    assert_error_contains(&r.errors, "display_name");
}

#[test]
fn classification_duplicate_names_are_errors() {
    let toml = r#"
schema_version = 1
[[classification]]
name         = "ferrite"
display_name = "Ferrite"
[[classification]]
name         = "ferrite"
display_name = "Ferrite copy"
"#;
    let r = validate_classification_file("cls.toml", toml);
    assert_error_contains(&r.errors, "duplicate classification name");
}

#[test]
fn classification_range_out_of_unit_interval_is_error() {
    let toml = r#"
schema_version = 1
[[classification]]
name         = "big_range"
display_name = "Big Range"
[classification.density]
min = -0.1
max = 1.0
"#;
    let r = validate_classification_file("cls.toml", toml);
    assert_error_contains(&r.errors, "outside valid range");
}

#[test]
fn classification_min_equal_to_max_warns_not_errors() {
    let toml = r#"
schema_version = 1
[[classification]]
name         = "pointrange"
display_name = "Point Range"
[classification.density]
min = 0.5
max = 0.5
"#;
    let r = validate_classification_file("cls.toml", toml);
    // This is a warning, not an error — the file can still be loaded.
    assert_no_errors(&r.errors);
    assert_warning_contains(&r.warnings, "never match");
}

#[test]
fn classification_overlapping_density_ranges_produce_warning() {
    let toml = r#"
schema_version = 1
[[classification]]
name         = "low_dense"
display_name = "Low Dense"
[classification.density]
min = 0.20
max = 0.50

[[classification]]
name         = "mid_dense"
display_name = "Mid Dense"
[classification.density]
min = 0.45
max = 0.75
"#;
    let r = validate_classification_file("cls.toml", toml);
    assert_no_errors(&r.errors);
    assert_warning_contains(&r.warnings, "overlap");
}

// ── Combinations: invalid inputs ──────────────────────────────────────────

#[test]
fn combinations_absent_seed_a_is_error() {
    let toml = r#"
schema_version = 1
[[rules]]
material_seed_b = 1001
"#;
    let r = validate_combinations_file("combo.toml", toml);
    assert_error_contains(&r.errors, "material_seed_a");
}

#[test]
fn combinations_absent_seed_b_is_error() {
    let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 9002
"#;
    let r = validate_combinations_file("combo.toml", toml);
    assert_error_contains(&r.errors, "material_seed_b");
}

#[test]
fn combinations_self_pairing_is_error() {
    let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 1001
material_seed_b = 1001
"#;
    let r = validate_combinations_file("combo.toml", toml);
    assert_error_contains(&r.errors, "self-combination");
}

#[test]
fn combinations_reversed_duplicate_pair_is_error() {
    // (9002, 1001) and (1001, 9002) are the same normalised pair.
    let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 9002
material_seed_b = 1001

[[rules]]
material_seed_a = 1001
material_seed_b = 9002
"#;
    let r = validate_combinations_file("combo.toml", toml);
    assert_error_contains(&r.errors, "duplicate rule");
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
    let r = validate_combinations_file("combo.toml", toml);
    assert_error_contains(&r.errors, "multiplier");
}

#[test]
fn combinations_catalyze_negative_multiplier_is_error() {
    let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 9002
material_seed_b = 1001
density = { type = "Catalyze", multiplier = -1.0 }
"#;
    let r = validate_combinations_file("combo.toml", toml);
    assert_error_contains(&r.errors, "multiplier");
}

#[test]
fn combinations_catalyze_large_multiplier_warns() {
    let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 9002
material_seed_b = 1001
density = { type = "Catalyze", multiplier = 12.0 }
"#;
    let r = validate_combinations_file("combo.toml", toml);
    assert_no_errors(&r.errors);
    assert_warning_contains(&r.warnings, "unusually large");
}

#[test]
fn combinations_blend_missing_weight_a_is_error() {
    let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 9002
material_seed_b = 1001
density = { type = "Blend", weight_b = 0.5 }
"#;
    let r = validate_combinations_file("combo.toml", toml);
    assert_error_contains(&r.errors, "weight_a");
}

#[test]
fn combinations_blend_zero_weight_b_is_error() {
    let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 9002
material_seed_b = 1001
density = { type = "Blend", weight_a = 1.0, weight_b = 0.0 }
"#;
    let r = validate_combinations_file("combo.toml", toml);
    assert_error_contains(&r.errors, "weight_b");
}

#[test]
fn combinations_unknown_rule_type_is_error() {
    let toml = r#"
schema_version = 1
[[rules]]
material_seed_a = 9002
material_seed_b = 1001
density = { type = "MagicBlend" }
"#;
    let r = validate_combinations_file("combo.toml", toml);
    assert_error_contains(&r.errors, "unknown PropertyRule type");
}

// ── Biomes: invalid inputs ─────────────────────────────────────────────────

#[test]
fn biomes_absent_schema_version_is_error() {
    let toml = r#"
[[biomes]]
biome_type = "test_biome"
temperature_min = 0.0
temperature_max = 1.0
moisture_min = 0.0
moisture_max = 1.0
"#;
    let r = validate_biomes_file("biomes.toml", toml);
    assert_error_contains(&r.errors, "schema_version");
}

#[test]
fn biomes_empty_biome_type_is_error() {
    let toml = r#"
schema_version = 1
[[biomes]]
biome_type      = ""
temperature_min = 0.0
temperature_max = 1.0
moisture_min    = 0.0
moisture_max    = 1.0
"#;
    let r = validate_biomes_file("biomes.toml", toml);
    assert_error_contains(&r.errors, "biome_type");
}

#[test]
fn biomes_temperature_min_exceeds_max_is_error() {
    let toml = r#"
schema_version = 1
[[biomes]]
biome_type      = "inverted"
temperature_min = 0.8
temperature_max = 0.2
moisture_min    = 0.0
moisture_max    = 1.0
"#;
    let r = validate_biomes_file("biomes.toml", toml);
    assert_error_contains(&r.errors, "temperature_min");
}

#[test]
fn biomes_moisture_min_equals_max_is_error() {
    let toml = r#"
schema_version = 1
[[biomes]]
biome_type      = "slim_moisture"
temperature_min = 0.0
temperature_max = 1.0
moisture_min    = 0.6
moisture_max    = 0.6
"#;
    let r = validate_biomes_file("biomes.toml", toml);
    assert_error_contains(&r.errors, "moisture_min");
}

#[test]
fn biomes_temperature_out_of_range_is_error() {
    let toml = r#"
schema_version = 1
[[biomes]]
biome_type      = "hot_biome"
temperature_min = 0.0
temperature_max = 1.5
moisture_min    = 0.0
moisture_max    = 1.0
"#;
    let r = validate_biomes_file("biomes.toml", toml);
    assert_error_contains(&r.errors, "outside valid range");
}

#[test]
fn biomes_zero_density_modifier_is_error() {
    let toml = r#"
schema_version = 1
[[biomes]]
biome_type       = "empty_biome"
temperature_min  = 0.0
temperature_max  = 1.0
moisture_min     = 0.0
moisture_max     = 1.0
density_modifier = 0.0
"#;
    let r = validate_biomes_file("biomes.toml", toml);
    assert_error_contains(&r.errors, "density_modifier");
}

#[test]
fn biomes_ground_color_wrong_length_is_error() {
    let toml = r#"
schema_version = 1
[[biomes]]
biome_type      = "two_color"
temperature_min = 0.0
temperature_max = 1.0
moisture_min    = 0.0
moisture_max    = 1.0
ground_color    = [0.5, 0.5]
"#;
    let r = validate_biomes_file("biomes.toml", toml);
    assert_error_contains(&r.errors, "ground_color");
}

#[test]
fn biomes_ground_color_component_above_one_is_error() {
    let toml = r#"
schema_version = 1
[[biomes]]
biome_type      = "bright"
temperature_min = 0.0
temperature_max = 1.0
moisture_min    = 0.0
moisture_max    = 1.0
ground_color    = [1.1, 0.5, 0.5]
"#;
    let r = validate_biomes_file("biomes.toml", toml);
    assert_error_contains(&r.errors, "ground_color");
}

#[test]
fn biomes_palette_missing_seed_is_error() {
    let toml = r#"
schema_version = 1
[[biomes]]
biome_type      = "seedy"
temperature_min = 0.0
temperature_max = 1.0
moisture_min    = 0.0
moisture_max    = 1.0

[[biomes.material_palette]]
selection_weight = 1.0
"#;
    let r = validate_biomes_file("biomes.toml", toml);
    assert_error_contains(&r.errors, "material_seed");
}

#[test]
fn biomes_palette_negative_weight_is_error() {
    let toml = r#"
schema_version = 1
[[biomes]]
biome_type      = "neg_weight"
temperature_min = 0.0
temperature_max = 1.0
moisture_min    = 0.0
moisture_max    = 1.0

[[biomes.material_palette]]
material_seed    = 1001
selection_weight = -1.0
"#;
    let r = validate_biomes_file("biomes.toml", toml);
    assert_error_contains(&r.errors, "selection_weight");
}

// ── Language: invalid inputs ───────────────────────────────────────────────

#[test]
fn language_absent_table_is_error() {
    let toml = r#"
schema_version = 1
# no [language] table at all
"#;
    let r = validate_language_file("lang.toml", toml);
    assert_error_contains(&r.errors, "[language]");
}

#[test]
fn language_missing_id_is_error() {
    let toml = r#"
schema_version = 1
[language]
display_name     = "Missing ID"
race_id          = "veth"
modality         = "gestural"
strangeness      = 0.5
acquisition_rate = 1.0
"#;
    let r = validate_language_file("lang.toml", toml);
    assert_error_contains(&r.errors, "'id'");
}

#[test]
fn language_strangeness_above_one_is_error() {
    let toml = r#"
schema_version = 1
[language]
id               = "ultra_weird"
display_name     = "Ultra Weird"
race_id          = "aliens"
modality         = "vocal"
strangeness      = 1.01
acquisition_rate = 1.0
"#;
    let r = validate_language_file("lang.toml", toml);
    assert_error_contains(&r.errors, "strangeness");
}

#[test]
fn language_negative_acquisition_rate_is_error() {
    let toml = r#"
schema_version = 1
[language]
id               = "unmakeable"
display_name     = "Unmakeable"
race_id          = "nobody"
modality         = "written"
strangeness      = 0.0
acquisition_rate = -1.0
"#;
    let r = validate_language_file("lang.toml", toml);
    assert_error_contains(&r.errors, "acquisition_rate");
}

// ── Race: invalid inputs ───────────────────────────────────────────────────

#[test]
fn race_absent_table_is_error() {
    let toml = r#"
schema_version = 1
# no [race] table
"#;
    let r = validate_race_file("race.toml", toml);
    assert_error_contains(&r.errors, "[race]");
}

#[test]
fn race_missing_id_is_error() {
    let toml = r#"
schema_version = 1
[race]
display_name = "Unknown Race"
language_id  = "common"
strangeness  = 0.5
"#;
    let r = validate_race_file("race.toml", toml);
    assert_error_contains(&r.errors, "'id'");
}

#[test]
fn race_missing_language_id_is_error() {
    let toml = r#"
schema_version = 1
[race]
id           = "mute_race"
display_name = "Mute Race"
strangeness  = 0.5
"#;
    let r = validate_race_file("race.toml", toml);
    assert_error_contains(&r.errors, "language_id");
}

#[test]
fn race_strangeness_below_zero_is_error() {
    let toml = r#"
schema_version = 1
[race]
id           = "nice"
display_name = "Nice"
language_id  = "common"
strangeness  = -0.5
"#;
    let r = validate_race_file("race.toml", toml);
    assert_error_contains(&r.errors, "strangeness");
}

// ── Cross-reference validation ─────────────────────────────────────────────

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
    assert_warning_contains(&r.warnings, "race_id");
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
language_id  = "other_lang"
strangeness  = 0.78
"#;
    let r = validate_language_race_crossref("lang.toml", lang, "race.toml", race);
    assert_warning_contains(&r.warnings, "language_id");
}

#[test]
fn crossref_matching_ids_produce_no_warnings() {
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

// ── Regression: real example-mod assets must all pass clean ───────────────

#[test]
fn example_crystal_lichen_classifications_passes() {
    let content = std::fs::read_to_string(
        "mods/example.crystal-lichen/assets/materials/classifications.toml",
    )
    .expect("read classifications.toml");
    let r = validate_classification_file(
        "mods/example.crystal-lichen/assets/materials/classifications.toml",
        &content,
    );
    assert_no_errors(&r.errors);
}

#[test]
fn example_crystal_lichen_biomes_passes() {
    let content = std::fs::read_to_string("mods/example.crystal-lichen/assets/config/biomes.toml")
        .expect("read biomes.toml");
    let r = validate_biomes_file(
        "mods/example.crystal-lichen/assets/config/biomes.toml",
        &content,
    );
    assert_no_errors(&r.errors);
}

#[test]
fn example_crystal_lichen_combinations_passes() {
    let content =
        std::fs::read_to_string("mods/example.crystal-lichen/assets/config/combinations.toml")
            .expect("read combinations.toml");
    let r = validate_combinations_file(
        "mods/example.crystal-lichen/assets/config/combinations.toml",
        &content,
    );
    assert_no_errors(&r.errors);
}

#[test]
fn example_glimmersteel_classifications_passes() {
    let content =
        std::fs::read_to_string("mods/example.glimmersteel/assets/materials/classifications.toml")
            .expect("read glimmersteel classifications.toml");
    let r = validate_classification_file(
        "mods/example.glimmersteel/assets/materials/classifications.toml",
        &content,
    );
    assert_no_errors(&r.errors);
}

#[test]
fn example_deep_sign_language_passes() {
    let content = std::fs::read_to_string("mods/example.deep-sign/assets/languages/deep_sign.toml")
        .expect("read deep_sign.toml");
    let r = validate_language_file(
        "mods/example.deep-sign/assets/languages/deep_sign.toml",
        &content,
    );
    assert_no_errors(&r.errors);
}

#[test]
fn example_deep_sign_race_passes() {
    let content = std::fs::read_to_string("mods/example.deep-sign/assets/races/veth.toml")
        .expect("read veth.toml");
    let r = validate_race_file("mods/example.deep-sign/assets/races/veth.toml", &content);
    assert_no_errors(&r.errors);
}

#[test]
fn example_deep_sign_crossref_passes() {
    let lang = std::fs::read_to_string("mods/example.deep-sign/assets/languages/deep_sign.toml")
        .expect("read deep_sign.toml");
    let race = std::fs::read_to_string("mods/example.deep-sign/assets/races/veth.toml")
        .expect("read veth.toml");
    let r = validate_language_race_crossref(
        "mods/example.deep-sign/assets/languages/deep_sign.toml",
        &lang,
        "mods/example.deep-sign/assets/races/veth.toml",
        &race,
    );
    assert!(
        r.warnings.is_empty(),
        "expected no crossref warnings, got: {:?}",
        r.warnings
    );
}
