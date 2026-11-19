//! Syntax colours for code from GPUI Kit's active highlight theme.
//! The GitHub diff view uses it.

use std::ops::Range;

use gpui_kit::HighlightStyle;
use gpui_kit::base::input::HighlightStyleResolver;
use gpui_kit::component::Rope;
use gpui_kit::component::highlighter::SyntaxHighlighter;

/// Style runs of one displayed line, byte ranges in its text.
pub type Runs = Vec<(Range<usize>, HighlightStyle)>;

/// The grammar for a path, by its extension or name; `None` for plain text.
pub fn language(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.eq_ignore_ascii_case("makefile") || name.eq_ignore_ascii_case("dockerfile") {
        return None;
    }
    let (_, extension) = name.rsplit_once('.')?;
    Some(match extension.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "py" | "pyi" => "python",
        "go" => "go",
        "sh" | "bash" | "zsh" => "bash",
        "yml" | "yaml" => "yaml",
        "toml" => "toml",
        "json" | "jsonc" => "json",
        "cs" => "csharp",
        "css" | "scss" => "css",
        "html" | "htm" => "html",
        "md" | "markdown" => "markdown",
        _ => return None,
    })
}

/// Style runs per line of `texts`, parsed as one text.
// qual:api
pub fn line_runs(
    language: &str,
    texts: &[String],
    theme: &dyn HighlightStyleResolver,
) -> Vec<Runs> {
    let text = texts.join("\n");
    let rope = Rope::from_str(&text);
    let mut highlighter = SyntaxHighlighter::new(language);
    highlighter.update(None, &rope, None);
    let styles = highlighter.styles(&(0..text.len()), theme);
    let mut out = Vec::with_capacity(texts.len());
    let mut start = 0usize;
    for line in texts {
        let end = start + line.len();
        let runs = styles
            .iter()
            .filter(|(range, style)| {
                range.start < end && range.end > start && style.color.is_some()
            })
            .map(|(range, style)| {
                (
                    range.start.max(start) - start..range.end.min(end) - start,
                    *style,
                )
            })
            .filter(|(range, _)| !range.is_empty())
            .collect();
        out.push(runs);
        start = end + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{language, line_runs};
    use gpui_kit::component::highlighter::HighlightTheme;

    #[test]
    fn languages_come_from_the_extension() {
        assert_eq!(language("src/main.rs"), Some("rust"));
        assert_eq!(language("a/b.test.TSX"), Some("tsx"));
        assert_eq!(language(".github/workflows/ci.yml"), Some("yaml"));
        assert_eq!(language("Makefile"), None);
        assert_eq!(language("README"), None);
    }

    #[test]
    fn rust_keywords_get_a_colour() {
        let runs = line_runs(
            "rust",
            &["fn main() {}".to_string(), "let x = 1;".to_string()],
            &*HighlightTheme::default_dark(),
        );
        assert_eq!(runs.len(), 2);
        assert!(
            runs[0].iter().any(|(range, _)| *range == (0..2)),
            "{:?}",
            runs[0]
        );
    }

    #[test]
    fn keyword_colours_follow_the_selected_palette() {
        let texts = ["fn main() {}".to_string()];
        let dark = line_runs("rust", &texts, &*HighlightTheme::default_dark());
        let light = line_runs("rust", &texts, &*HighlightTheme::default_light());
        let keyword = |runs: &Vec<super::Runs>| {
            runs[0]
                .iter()
                .find(|(range, _)| *range == (0..2))
                .unwrap()
                .1
        };
        assert_ne!(keyword(&dark).color, keyword(&light).color);
        assert_eq!(
            dark[0].iter().map(|(range, _)| range).collect::<Vec<_>>(),
            light[0].iter().map(|(range, _)| range).collect::<Vec<_>>()
        );
    }
}
