//! Font roles resolve independently and never require font downloads.
//!
//! The family matching itself lives in [`subspace_dashboard::typography`], so
//! the terminal panel resolves the Ghostty `font-family` lists against the
//! same installed families (REQ-14).

use subspace_dashboard::typography::resolve_role;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedFonts {
    pub heading: String,
    pub body: String,
    pub mono: String,
    pub warnings: Vec<String>,
}

pub(crate) fn resolve(
    heading: &str,
    body: &str,
    mono: &str,
    installed: &[String],
) -> ResolvedFonts {
    let sans = automatic_sans(installed);
    let code = automatic_mono(installed);
    let mut warnings = Vec::new();
    let body = resolve_role("Body text", body, &sans, installed, &mut warnings);
    let mono = resolve_role("Code and logs", mono, &code, installed, &mut warnings);
    let heading = resolve_role(
        "Headings and tabs",
        heading,
        &body,
        installed,
        &mut warnings,
    );
    ResolvedFonts {
        heading,
        body,
        mono,
        warnings,
    }
}

pub(crate) fn resolve_group_heading(
    requested: &str,
    fallback: &str,
    installed: &[String],
    warnings: &mut Vec<String>,
) -> String {
    resolve_role("Group headings", requested, fallback, installed, warnings)
}

fn first_installed(candidates: &[&str], installed: &[String]) -> String {
    candidates
        .iter()
        .find_map(|candidate| {
            installed
                .iter()
                .find(|font| font.as_str() == *candidate)
                .cloned()
        })
        .unwrap_or_else(|| ".SystemUIFont".into())
}

fn automatic_sans(installed: &[String]) -> String {
    first_installed(
        &[
            ".SystemUIFont",
            "Segoe UI",
            "Helvetica Neue",
            "Arial",
            "Noto Sans",
            "DejaVu Sans",
            "Liberation Sans",
        ],
        installed,
    )
}

fn automatic_mono(installed: &[String]) -> String {
    first_installed(
        &[
            "JetBrains Mono",
            "SF Mono",
            "Menlo",
            "Consolas",
            "DejaVu Sans Mono",
            "Liberation Mono",
            "Noto Sans Mono",
            "Ubuntu Mono",
        ],
        installed,
    )
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn a_heading_family_does_not_change_body_or_mono() {
        let fonts = ["Inter", "Arial", "Menlo"].map(str::to_owned);
        let resolved = resolve("Inter", "Arial", "Menlo", &fonts);
        assert_eq!(resolved.heading, "Inter");
        assert_eq!(resolved.body, "Arial");
        assert_eq!(resolved.mono, "Menlo");
        assert!(resolved.warnings.is_empty());
    }

    #[test]
    fn missing_roles_fall_back_independently_and_report_the_role() {
        let fonts = ["Arial", "Consolas"].map(str::to_owned);
        let resolved = resolve("Missing Display", "Missing Sans", "Missing Mono", &fonts);
        assert_eq!(resolved.heading, "Arial");
        assert_eq!(resolved.body, "Arial");
        assert_eq!(resolved.mono, "Consolas");
        assert_eq!(resolved.warnings.len(), 3);
        assert!(resolved.warnings[0].contains("Body"));
        assert!(
            resolved
                .warnings
                .iter()
                .any(|warning| warning.contains("Headings"))
        );
    }
}
