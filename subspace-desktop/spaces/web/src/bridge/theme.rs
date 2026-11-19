//! The app's theme as a page sees it: the resolved tones as `#rrggbb`, the
//! mode, the theme's id and name, and CSS font families. Reading it needs no
//! permission; it says nothing the page could not see on screen.

use gpui_kit::component::Theme;
use gpui_kit::{App, Hsla, Rgba};
use serde_json::{Value, json};
use subspace_framework::dashboard::contrast::composite;
use subspace_framework::dashboard::{ThemeId, Tokens};

/// Fallbacks after the app's interface font, which WebKit may not have.
const UI_FALLBACK: &str = "system-ui, -apple-system, sans-serif";
/// Fallbacks after the app's monospaced font.
const MONO_FALLBACK: &str = "ui-monospace, monospace";
const MAX_CHANNEL: f32 = 255.;

/// What the page's theme is made of, read from the app's globals.
pub(crate) struct ThemeFacts<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub dark: bool,
    pub tokens: Tokens,
    pub ui_font: &'a str,
    pub mono_font: &'a str,
}

/// The theme in effect, as the page receives it.
pub(crate) fn current(cx: &App) -> Value {
    let theme = Theme::global(cx);
    let id = cx
        .try_global::<ThemeId>()
        .map_or("custom", |id| id.0.as_str());
    to_json(&ThemeFacts {
        id: if id.is_empty() { "custom" } else { id },
        name: theme.theme_name(),
        dark: theme.is_dark(),
        tokens: Tokens::current(cx),
        ui_font: &theme.font_family,
        mono_font: &theme.mono_font_family,
    })
}

/// The documented `Theme` shape. Translucent tones are flattened onto the
/// opaque background, so every tone is a plain `#rrggbb`.
pub(crate) fn to_json(facts: &ThemeFacts<'_>) -> Value {
    let tokens = &facts.tokens;
    let mut under = tokens.bg.to_rgb();
    under.a = 1.;
    let tone = |color: Hsla| hex(composite(color.to_rgb(), under));
    json!({
        "id": facts.id,
        "name": facts.name,
        "mode": if facts.dark { "dark" } else { "light" },
        "tones": {
            "bg": hex(under), "pane": tone(tokens.pane), "line": tone(tokens.line),
            "hair": tone(tokens.hair), "select": tone(tokens.select), "fg": tone(tokens.fg),
            "strong": tone(tokens.fg_strong), "muted": tone(tokens.muted),
            "accent": tone(tokens.accent), "red": tone(tokens.red),
            "orange": tone(tokens.orange), "yellow": tone(tokens.yellow),
            "green": tone(tokens.green), "cyan": tone(tokens.cyan),
            "blue": tone(tokens.blue), "magenta": tone(tokens.magenta),
        },
        "fonts": {
            "ui": font_family(facts.ui_font, UI_FALLBACK),
            "mono": font_family(facts.mono_font, MONO_FALLBACK),
        },
    })
}

fn hex(color: Rgba) -> String {
    let channel = |value: f32| (value.clamp(0., 1.) * MAX_CHANNEL).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(color.r),
        channel(color.g),
        channel(color.b)
    )
}

/// A CSS `font-family` value: the app's family, quoted, then generic
/// fallbacks. GPUI's virtual families (such as `.SystemUIFont`) start with
/// a dot and become the fallbacks alone.
fn font_family(name: &str, fallback: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    if name.is_empty() || name.starts_with('.') {
        return fallback.to_owned();
    }
    let quoted = name.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{quoted}\", {fallback}")
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Hsla, rgb, rgba};
    use serde_json::json;
    use subspace_framework::dashboard::Tokens;

    use super::{ThemeFacts, font_family, to_json};

    fn facts(tokens: Tokens) -> ThemeFacts<'static> {
        ThemeFacts {
            id: "one-dark",
            name: "Subspace One Dark",
            dark: true,
            tokens,
            ui_font: ".SystemUIFont",
            mono_font: "Mono \"Pro\"",
        }
    }

    #[test]
    fn the_theme_has_every_documented_tone_as_rrggbb() {
        let value = to_json(&facts(Tokens::one_dark()));
        assert_eq!(value["id"], "one-dark");
        assert_eq!(value["name"], "Subspace One Dark");
        assert_eq!(value["mode"], "dark");
        assert_eq!(value["tones"]["bg"], "#101115");
        assert_eq!(value["tones"]["accent"], "#7dd3fc");
        assert_eq!(value["tones"]["strong"], "#d1d5dc");
        let tones = value["tones"].as_object().unwrap();
        let names: Vec<&str> = tones.keys().map(String::as_str).collect();
        let expected = [
            "bg", "pane", "line", "hair", "select", "fg", "strong", "muted", "accent", "red",
            "orange", "yellow", "green", "cyan", "blue", "magenta",
        ];
        let mut sorted = names.clone();
        sorted.sort_unstable();
        let mut documented = expected;
        documented.sort_unstable();
        assert_eq!(sorted, documented);
        for tone in tones.values() {
            let tone = tone.as_str().unwrap();
            assert!(tone.len() == 7 && tone.starts_with('#'), "{tone}");
            assert!(tone[1..].bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn translucent_tones_are_flattened_onto_the_background() {
        let mut tokens = Tokens::one_dark();
        tokens.bg = Hsla::from(rgb(0x000000));
        tokens.select = Hsla::from(rgba(0xffffff80));
        let mut light = facts(tokens);
        light.dark = false;
        let value = to_json(&light);
        assert_eq!(value["mode"], "light");
        assert_eq!(value["tones"]["select"], "#808080");
    }

    #[test]
    fn fonts_are_css_families_with_fallbacks() {
        let value = to_json(&facts(Tokens::one_dark()));
        assert_eq!(
            value["fonts"],
            json!({
                "ui": "system-ui, -apple-system, sans-serif",
                "mono": "\"Mono \\\"Pro\\\"\", ui-monospace, monospace",
            })
        );
        assert_eq!(font_family("Inter\n", "serif"), "\"Inter\", serif");
        assert_eq!(font_family("", "serif"), "serif");
    }
}
