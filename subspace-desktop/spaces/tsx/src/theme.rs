//! The theme hand-off: the app's tokens as nsh's shadcn/ui tokens, so a
//! package that uses only token classes draws in the Subspace theme. Runs
//! again on every theme change. The custom `text-edge` token is the design
//! guide's text edge, where every panel starts its text.

use gpui_kit::{Hsla, Pixels, Rgba};
use serde_json::{Map, Value, json};
use subspace_framework::dashboard::Tokens;

const MAX_CHANNEL: f32 = 255.;
const OPAQUE: f32 = 1.;
const TRANSPARENT: f32 = 0.;

/// The theme file nsh applies over a package's own theme: the colours in
/// both modes (the host sets which one shows) and the shared sizes.
pub(crate) fn css_vars(tokens: &Tokens, text_edge: Pixels) -> Value {
    let colours = colours(tokens);
    json!({
        "cssVars": {
            "theme": {
                "radius": "6px",
                "text-edge": format!("{}px", f32::from(text_edge)),
            },
            "light": colours,
            "dark": colours,
        }
    })
}

fn colours(tokens: &Tokens) -> Map<String, Value> {
    [
        ("background", tokens.bg),
        ("foreground", tokens.fg),
        ("card", tokens.pane),
        ("card-foreground", tokens.fg),
        ("popover", tokens.surface),
        ("popover-foreground", tokens.fg),
        ("primary", tokens.accent),
        ("primary-foreground", tokens.bg),
        ("secondary", tokens.select),
        ("secondary-foreground", tokens.fg_strong),
        ("muted", tokens.pane),
        ("muted-foreground", tokens.muted),
        ("accent", tokens.select),
        ("accent-foreground", tokens.fg_strong),
        ("destructive", tokens.red),
        ("border", tokens.line),
        ("input", tokens.line),
        ("ring", tokens.accent),
        ("chart-1", tokens.accent),
        ("chart-2", tokens.green),
        ("chart-3", tokens.orange),
        ("chart-4", tokens.magenta),
        ("chart-5", tokens.cyan),
    ]
    .into_iter()
    .map(|(name, colour)| (name.to_owned(), Value::String(hex(colour))))
    .collect()
}

/// `#rrggbb`, or `#rrggbbaa` for a translucent tone.
fn hex(colour: Hsla) -> String {
    let Rgba { r, g, b, a } = colour.to_rgb();
    let byte = |channel: f32| (channel.clamp(TRANSPARENT, OPAQUE) * MAX_CHANNEL).round() as u8;
    if a >= OPAQUE {
        format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            byte(r),
            byte(g),
            byte(b),
            byte(a)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{css_vars, hex};
    use gpui_kit::{hsla, px};
    use subspace_framework::dashboard::Tokens;

    #[test]
    fn colours_are_hex_and_the_text_edge_is_a_size() {
        let mut tokens = Tokens::one_dark();
        tokens.bg = hsla(0., 0., 1., 1.);
        let vars = css_vars(&tokens, px(16.));
        assert_eq!(vars["cssVars"]["light"]["background"], "#ffffff");
        assert_eq!(vars["cssVars"]["dark"]["background"], "#ffffff");
        assert_eq!(vars["cssVars"]["theme"]["text-edge"], "16px");
    }

    #[test]
    fn a_translucent_tone_keeps_its_alpha() {
        assert_eq!(hex(hsla(0., 0., 0., 0.5)), "#00000080");
    }
}
