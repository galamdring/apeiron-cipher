//! Player-facing descriptive language — translates raw numeric values into
//! qualitative prose the player reads in the journal, examine panel, and
//! other UI surfaces.
//!
//! Centralising these functions avoids duplication across modules and keeps
//! the "voice" of the game consistent. Every function here converts a
//! normalised `0.0–1.0` value (or similar) into a `&'static str` or
//! `String` that can go straight into UI text.

use crate::observation::ConfidenceLevel;

// ── Generic value ───────────────────────────────────────────────────────

/// Converts a normalised 0–1 property value into a descriptive word.
pub fn describe_value(value: f32) -> &'static str {
    if value < 0.15 {
        "Negligible"
    } else if value < 0.3 {
        "Very low"
    } else if value < 0.45 {
        "Low"
    } else if value < 0.55 {
        "Moderate"
    } else if value < 0.7 {
        "High"
    } else if value < 0.85 {
        "Very high"
    } else {
        "Extreme"
    }
}

// ── Density / weight ────────────────────────────────────────────────────

/// Converts a normalised density value into a weight description.
pub fn describe_density(value: f32) -> &'static str {
    if value < 0.15 {
        "Almost weightless"
    } else if value < 0.3 {
        "Very light"
    } else if value < 0.45 {
        "Light"
    } else if value < 0.55 {
        "Medium weight"
    } else if value < 0.7 {
        "Heavy"
    } else if value < 0.85 {
        "Very heavy"
    } else {
        "Extremely dense"
    }
}

// ── Color ───────────────────────────────────────────────────────────────

/// Converts an RGB triplet into a qualitative colour description.
pub fn describe_color(color: &[f32; 3]) -> &'static str {
    let [r, g, b] = *color;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);

    if max - min < 0.08 {
        if max < 0.25 {
            "Dark mineral grey"
        } else if max < 0.7 {
            "Muted stone grey"
        } else {
            "Pale chalk grey"
        }
    } else if r >= g && r >= b {
        "Warm rust tone"
    } else if g >= r && g >= b {
        "Verdant green tone"
    } else {
        "Cool blue tone"
    }
}

// ── Thermal ─────────────────────────────────────────────────────────────

/// Base thermal-behaviour description without confidence framing.
fn describe_thermal_behavior(value: f32) -> &'static str {
    if value < 0.25 {
        "soften quickly under heat"
    } else if value < 0.5 {
        "change noticeably under heat"
    } else if value < 0.75 {
        "hold together under heat"
    } else {
        "barely react to heat"
    }
}

/// Thermal observation with confidence-level framing applied.
pub fn describe_thermal_observation(value: f32, confidence: ConfidenceLevel) -> String {
    let behavior = describe_thermal_behavior(value);
    match confidence {
        ConfidenceLevel::Tentative => format!("Seemed to {behavior}"),
        ConfidenceLevel::Observed => {
            let mut chars = behavior.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            format!("{}{}", first.to_uppercase(), chars.as_str())
        }
        ConfidenceLevel::Confident => format!("Reliably {behavior}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_value_covers_full_range() {
        assert_eq!(describe_value(0.0), "Negligible");
        assert_eq!(describe_value(0.2), "Very low");
        assert_eq!(describe_value(0.35), "Low");
        assert_eq!(describe_value(0.5), "Moderate");
        assert_eq!(describe_value(0.6), "High");
        assert_eq!(describe_value(0.75), "Very high");
        assert_eq!(describe_value(0.9), "Extreme");
    }

    #[test]
    fn describe_density_covers_full_range() {
        assert_eq!(describe_density(0.1), "Almost weightless");
        assert_eq!(describe_density(0.25), "Very light");
        assert_eq!(describe_density(0.4), "Light");
        assert_eq!(describe_density(0.5), "Medium weight");
        assert_eq!(describe_density(0.65), "Heavy");
        assert_eq!(describe_density(0.78), "Very heavy");
        assert_eq!(describe_density(0.95), "Extremely dense");
    }
}
