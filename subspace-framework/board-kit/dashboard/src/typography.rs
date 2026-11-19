//! Typography roles shared by the app shell and dashboard widgets.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{App, FontWeight, Global, SharedString, Styled, px};

pub const GROUP_HEADING_SIZE_DEFAULT: f32 = 13.;
pub const GROUP_HEADING_SIZE_MIN: f32 = 10.;
pub const GROUP_HEADING_SIZE_MAX: f32 = 24.;

pub fn format_group_heading_size(size: f32) -> String {
    format!("{size} px")
}

/// Typography for the title text of a collapsible row group.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupHeading {
    pub font: SharedString,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

impl Default for GroupHeading {
    fn default() -> Self {
        Self {
            font: SharedString::default(),
            size: GROUP_HEADING_SIZE_DEFAULT,
            bold: true,
            italic: false,
        }
    }
}

/// Font choices that are not represented by GPUI Kit's body and mono roles.
#[derive(Clone, Default)]
pub struct Typography {
    pub heading: SharedString,
    pub group_heading: GroupHeading,
}

impl Global for Typography {}

/// The installed family matching `requested`, compared without case.
pub fn installed_family(requested: &str, installed: &[String]) -> Option<String> {
    installed
        .iter()
        .find(|font| font.eq_ignore_ascii_case(requested))
        .cloned()
}

/// The first installed family of a list of requested families, in order.
///
/// The Ghostty `font-family` keys are lists: the terminal panel asks for each
/// family in turn and takes the first the host has installed.
// qual:api
pub fn first_installed_family(requested: &[String], installed: &[String]) -> Option<String> {
    requested
        .iter()
        .find_map(|family| installed_family(family, installed))
}

/// The family for one role: the requested family when it is installed, else
/// `fallback` with a warning naming the role. An empty request is not a
/// failure; it means the role is unset and takes the fallback silently.
pub fn resolve_role(
    role: &str,
    requested: &str,
    fallback: &str,
    installed: &[String],
    warnings: &mut Vec<String>,
) -> String {
    if requested.is_empty() {
        return fallback.to_owned();
    }
    if let Some(font) = installed_family(requested, installed) {
        return font;
    }
    warnings.push(format!(
        "{role}: {requested} is not installed; using {fallback}"
    ));
    fallback.to_owned()
}

/// The heading family, falling back to the active theme's body family.
pub fn heading_font(cx: &App) -> SharedString {
    cx.try_global::<Typography>()
        .map(|typography| typography.heading.clone())
        .filter(|heading| !heading.is_empty())
        .unwrap_or_else(|| cx.theme().font_family.clone())
}

/// Applies the group-heading role without affecting adjacent icons or counts.
pub fn group_heading<T: Styled + FluentBuilder>(element: T, cx: &App) -> T {
    let style = cx
        .try_global::<Typography>()
        .map(|typography| typography.group_heading.clone())
        .unwrap_or_default();
    let font = if style.font.is_empty() {
        heading_font(cx)
    } else {
        style.font
    };
    element
        .font_family(font)
        .text_size(px(style.size))
        .font_weight(if style.bold {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        })
        .when(style.italic, |element| element.italic())
}

#[cfg(test)]
mod tests {
    use super::{
        GroupHeading, first_installed_family, format_group_heading_size, installed_family,
        resolve_role,
    };

    #[test]
    fn an_installed_family_matches_without_case() {
        let installed = ["Inter".to_owned(), "Arial".to_owned()];

        assert_eq!(
            installed_family("inter", &installed),
            Some("Inter".to_owned())
        );
        assert_eq!(installed_family("Missing Sans", &installed), None);
    }

    #[test]
    fn a_family_list_takes_the_first_installed_family() {
        let installed = ["Menlo".to_owned(), "Arial".to_owned()];
        let requested = ["Missing Mono".to_owned(), "menlo".to_owned()];

        assert_eq!(
            first_installed_family(&requested, &installed),
            Some("Menlo".to_owned())
        );
        assert_eq!(first_installed_family(&[], &installed), None);
    }

    #[test]
    fn a_missing_role_falls_back_and_warns_once() {
        let installed = ["Menlo".to_owned()];
        let mut warnings = Vec::new();

        let unset = resolve_role("Code", "", "Menlo", &installed, &mut warnings);
        let missing = resolve_role("Code", "Missing Mono", "Menlo", &installed, &mut warnings);

        assert_eq!(unset, "Menlo");
        assert_eq!(missing, "Menlo");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Missing Mono"), "{:?}", warnings[0]);
    }

    #[test]
    fn group_heading_defaults_and_sizes_are_stable() {
        let style = GroupHeading::default();
        assert_eq!(style.size, 13.0);
        assert!(style.bold);
        assert!(!style.italic);
        assert_eq!(format_group_heading_size(style.size), "13 px");
        assert_eq!(format_group_heading_size(14.5), "14.5 px");
    }
}
