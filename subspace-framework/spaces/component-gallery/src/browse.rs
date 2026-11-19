//! What the gallery shows: the chosen section, the name filter and the
//! "Hide unused GPUI Kit components" choice. Pure functions, tested without a
//! window.

use crate::catalog::{Item, SECTIONS, Section, items};

/// The most cards a filter shows at once; refine the filter to see others.
pub(crate) const MAX_MATCHES: usize = 40;

/// The choices that decide which cards are visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Browse<'a> {
    pub(crate) query: &'a str,
    pub(crate) hide_unused: bool,
}

impl Browse<'_> {
    /// Whether `item` passes the toggle and every word of the query.
    pub(crate) fn shows(&self, item: &Item) -> bool {
        !(self.hide_unused && item.is_unused_kit()) && matches(item, self.query)
    }

    /// The visible items of `section`, in display order.
    pub(crate) fn items(&self, section: &'static Section) -> Vec<&'static Item> {
        section
            .items
            .iter()
            .filter(|item| self.shows(item))
            .collect()
    }

    /// The sections with a visible item for the query, each with its
    /// visible items, at most [`MAX_MATCHES`] items in all, and the total.
    pub(crate) fn matches(&self) -> (Vec<(&'static Section, Vec<&'static Item>)>, usize) {
        let mut left = MAX_MATCHES;
        let mut total = 0;
        let mut groups = Vec::new();
        for section in &SECTIONS {
            let items = self.items(section);
            total += items.len();
            let shown: Vec<_> = items.into_iter().take(left).collect();
            left -= shown.len();
            if !shown.is_empty() {
                groups.push((section, shown));
            }
        }
        (groups, total)
    }
}

/// Case-insensitive: every word of `query` is in the item's name, source or,
/// for a GPUI Kit component, one of its type paths (so `ButtonGroup` finds
/// the Button card).
pub(crate) fn matches(item: &Item, query: &str) -> bool {
    let paths = item
        .kit
        .as_ref()
        .map(|kit| kit.paths.join(" "))
        .unwrap_or_default();
    let haystack = format!("{} {} {paths}", item.name, item.source).to_lowercase();
    query
        .split_whitespace()
        .all(|word| haystack.contains(&word.to_lowercase()))
}

/// The section with `id`, else the first one.
pub(crate) fn section(id: &str) -> &'static Section {
    SECTIONS
        .iter()
        .find(|section| section.id == id)
        .unwrap_or(&SECTIONS[0])
}

/// How many GPUI Kit components the gallery shows and how many Subspace uses.
pub(crate) fn kit_counts() -> (usize, usize) {
    items()
        .filter(|item| item.kit.is_some())
        .fold((0, 0), |(total, used), item| {
            (total + 1, used + usize::from(!item.is_unused_kit()))
        })
}

#[cfg(test)]
mod tests {
    use super::{Browse, MAX_MATCHES, kit_counts, matches, section};
    use crate::catalog::{Library, SECTIONS, items};

    const ALL: Browse<'static> = Browse {
        query: "",
        hide_unused: false,
    };

    #[test]
    fn hiding_unused_removes_exactly_the_unused_kit_components() {
        let hidden = Browse {
            hide_unused: true,
            ..ALL
        };
        for section in &SECTIONS {
            let all = ALL.items(section);
            let shown = hidden.items(section);
            let unused = all.iter().filter(|item| item.is_unused_kit()).count();
            assert_eq!(shown.len(), all.len() - unused, "{}", section.id);
            assert!(shown.iter().all(|item| !item.is_unused_kit()));
            if section.library != Library::Kit {
                assert_eq!(shown.len(), all.len(), "{}", section.id);
            }
        }
        let (total, used) = kit_counts();
        let shown: usize = SECTIONS
            .iter()
            .filter(|section| section.library == Library::Kit)
            .map(|section| hidden.items(section).len())
            .sum();
        assert_eq!(shown, used);
        assert!(used <= total);
    }

    #[test]
    fn the_filter_needs_every_word_in_the_name_or_source_ignoring_case() {
        let Some(item) = items().find(|item| item.name == "action::icon_button") else {
            panic!("action::icon_button is in the gallery");
        };
        assert!(matches(item, ""));
        assert!(matches(item, "ICON action"));
        assert!(matches(item, "dashboard/src/action"));
        assert!(!matches(item, "icon tabs"));
        let Some(button) = items().find(|item| item.name == "gpui_kit::component::button::Button")
        else {
            panic!("the Kit Button is in the gallery");
        };
        assert!(matches(button, "buttongroup"));
    }

    #[test]
    fn matches_are_capped_and_counted() {
        let (groups, total) = ALL.matches();
        let shown: usize = groups.iter().map(|(_, items)| items.len()).sum();
        assert_eq!(total, items().count());
        assert_eq!(shown, MAX_MATCHES.min(total));
        assert!(groups.iter().all(|(_, items)| !items.is_empty()));
    }

    #[test]
    fn an_unknown_section_falls_back_to_the_first() {
        assert_eq!(section("no-such-section").id, SECTIONS[0].id);
        assert_eq!(section("kit-basic").id, "kit-basic");
    }
}
