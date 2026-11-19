//! WCAG contrast for text, focus indicators and control outlines after sRGB
//! alpha compositing. The theme designer and the switch track rule use it.

use gpui_kit::{Rgba, component::try_parse_color};

// WCAG 2 relative luminance and sRGB transfer-function constants.
const BLACK_LEVEL: f64 = 0.05;
const RED_WEIGHT: f64 = 0.2126;
const GREEN_WEIGHT: f64 = 0.7152;
const BLUE_WEIGHT: f64 = 0.0722;
const SRGB_LINEAR_THRESHOLD: f64 = 0.04045;
const SRGB_LINEAR_DIVISOR: f64 = 12.92;
const SRGB_OFFSET: f64 = 0.055;
const SRGB_SCALE: f64 = 1.055;
const SRGB_EXPONENT: f64 = 2.4;

/// Composites foreground over an opaque background before measuring contrast.
// qual:api - theme checks measure page-level pairs with it.
pub fn ratio(foreground: &str, background: &str) -> Result<f64, String> {
    let background = opaque(parse(background)?)?;
    Ok(contrast(
        composite(parse(foreground)?, background),
        background,
    ))
}

/// Composites background over an opaque base, then foreground over that result.
pub fn ratio_on(fg: &str, bg: &str, base: &str) -> Result<f64, String> {
    let background = composite(parse(bg)?, opaque(parse(base)?)?);
    Ok(contrast(composite(parse(fg)?, background), background))
}

/// Composites `foreground` over an opaque `background` and measures the pair.
pub fn ratio_over(foreground: Rgba, background: Rgba) -> f64 {
    contrast(composite(foreground, background), background)
}

fn parse(value: &str) -> Result<Rgba, String> {
    let color = try_parse_color(value).map_err(|error| error.to_string())?;
    validate(color.into())
}

fn validate(color: Rgba) -> Result<Rgba, String> {
    if [color.r, color.g, color.b, color.a]
        .iter()
        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
    {
        Ok(color)
    } else {
        Err("Contrast requires finite color channels between 0 and 1".into())
    }
}

fn opaque(color: Rgba) -> Result<Rgba, String> {
    if color.a == 1.0 {
        Ok(color)
    } else {
        Err("Contrast requires an opaque background or base color".into())
    }
}

pub fn composite(foreground: Rgba, background: Rgba) -> Rgba {
    let remaining = 1.0 - foreground.a;
    Rgba {
        r: foreground.r * foreground.a + background.r * remaining,
        g: foreground.g * foreground.a + background.g * remaining,
        b: foreground.b * foreground.a + background.b * remaining,
        a: 1.0,
    }
}

fn contrast(foreground: Rgba, background: Rgba) -> f64 {
    luminance_ratio(luminance(foreground), luminance(background))
}

fn luminance_ratio(first: f64, second: f64) -> f64 {
    (first.max(second) + BLACK_LEVEL) / (first.min(second) + BLACK_LEVEL)
}

fn luminance(color: Rgba) -> f64 {
    weighted_luminance([linear(color.r), linear(color.g), linear(color.b)])
}

fn weighted_luminance([red, green, blue]: [f64; 3]) -> f64 {
    RED_WEIGHT * red + GREEN_WEIGHT * green + BLUE_WEIGHT * blue
}

fn linear(channel: f32) -> f64 {
    let channel = f64::from(channel);
    if channel <= SRGB_LINEAR_THRESHOLD {
        channel / SRGB_LINEAR_DIVISOR
    } else {
        ((channel + SRGB_OFFSET) / SRGB_SCALE).powf(SRGB_EXPONENT)
    }
}

#[cfg(test)]
mod tests {
    use super::{ratio, ratio_on};

    #[test]
    fn opaque_contrast_uses_relative_luminance() {
        assert!((ratio("#000", "#fff").unwrap() - 21.0).abs() < 1e-6);
        assert!((ratio("#fff", "#000").unwrap() - 21.0).abs() < 1e-6);
        assert!((ratio("#ff0000", "#fff").unwrap() - 3.998_476_77).abs() < 1e-6);
        assert!((ratio("#777", "#777").unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn foreground_alpha_is_composited_before_linearization() {
        assert!((ratio("black/50", "white").unwrap() - 3.976_653).abs() < 1e-5);
        assert!((ratio("#0000", "#fff").unwrap() - 1.0).abs() < 1e-6);
        assert!((ratio("#00000080", "#fff").unwrap() - 4.004_106_96).abs() < 1e-5);
    }

    #[test]
    fn selection_background_is_composited_before_foreground() {
        let contrast = ratio_on("white/50", "black/50", "white").unwrap();
        assert!((contrast - 2.168_304_35).abs() < 1e-5);
        assert!((ratio_on("black", "white/0", "white").unwrap() - 21.0).abs() < 1e-6);
    }

    #[test]
    fn transparent_background_requires_an_opaque_base() {
        assert!(ratio("black", "white/50").is_err());
        assert!(ratio_on("black", "white/50", "white/50").is_err());
    }

    #[test]
    fn invalid_colors_return_errors() {
        assert!(ratio("not-a-color", "white").is_err());
        assert!(ratio("black", "#xyz").is_err());
        assert!(ratio("black/NaN", "white").is_err());
        assert!(ratio_on("black", "white", "invalid").is_err());
    }
}
