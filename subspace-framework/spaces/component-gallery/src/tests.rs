use std::collections::HashSet;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use subspace_dashboard::Dashboard as _;

use crate::ComponentGallery;
use crate::browse::Browse;
use crate::catalog::{Library, SECTIONS, items};
use crate::view::named;

mod catalogue;

#[test]
fn every_item_has_a_unique_non_empty_name() {
    let mut names = HashSet::new();
    for item in items() {
        assert!(!item.name.is_empty());
        assert!(
            !item.name.contains(char::is_whitespace),
            "{} has whitespace",
            item.name
        );
        assert!(!item.source.is_empty(), "{} has no source", item.name);
        assert!(!item.used_in.is_empty(), "{} has no use note", item.name);
        assert!(names.insert(item.name), "{} is not unique", item.name);
    }
    assert!(SECTIONS.iter().all(|section| !section.items.is_empty()));
}

/// A card's "In Subspace, use" link goes to a shared component in the gallery.
#[test]
fn every_preferred_treatment_is_a_subspace_card() {
    let shared: HashSet<_> = SECTIONS
        .iter()
        .filter(|section| section.library == Library::Subspace)
        .flat_map(|section| section.items)
        .map(|item| item.name)
        .collect();
    for item in items() {
        let prefer = item.kit.as_ref().and_then(|kit| kit.prefer);
        assert!(
            prefer.is_none_or(|name| shared.contains(name)),
            "{} prefers {prefer:?}, which is not a Subspace card",
            item.name
        );
    }
}

#[test]
fn sections_are_grouped_by_library_in_order() {
    let libraries: Vec<_> = SECTIONS.iter().map(|section| section.library).collect();
    let mut sorted = libraries.clone();
    sorted.sort_by_key(|library| Library::ALL.iter().position(|each| each == library));
    assert_eq!(libraries, sorted);
    let kit: Vec<_> = SECTIONS
        .iter()
        .filter(|section| section.library == Library::Kit)
        .map(|section| section.title)
        .collect();
    assert_eq!(
        kit,
        [
            "Basic components",
            "Form components",
            "Layout components",
            "Advanced components"
        ]
    );
}

#[cfg(feature = "test-support")]
#[gpui_kit::test]
fn fixture_card_uses_the_registered_item_and_rejects_unknown_names(cx: &mut TestAppContext) {
    let (gallery, mut cx) = open_gallery(cx);
    cx.update(|window, cx| {
        gallery.update(cx, |gallery, cx| {
            assert!(
                gallery
                    .render_test_item("copy_details::CopyDetails", window, cx)
                    .is_some()
            );
            assert!(
                gallery
                    .render_test_item("not-a-component", window, cx)
                    .is_none()
            );
        });
    });
}

/// Opens the production view in a default headless window, as the shell does.
pub(crate) fn open_gallery(
    cx: &mut TestAppContext,
) -> (Entity<ComponentGallery>, VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        subspace_dashboard::bind_keys(cx);
        ComponentGallery::bind_keys(cx);
    });
    let (window, gallery) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            cx.new(|cx| ComponentGallery::new(window, cx))
        })
        .unwrap()
    });
    let cx = VisualTestContext::from_window(window, cx);
    cx.run_until_parked();
    (gallery, cx)
}

/// Shows each section in turn. Prepaint registers every observed element,
/// including cards below the scrolled viewport, so one frame finds them all.
#[gpui_kit::test]
fn every_section_renders_each_of_its_cards(cx: &mut TestAppContext) {
    let (gallery, mut cx) = open_gallery(cx);
    for section in &SECTIONS {
        cx.update(|window, cx| {
            gallery.update(cx, |gallery, cx| {
                gallery.select_section(section.id, window, cx)
            });
            window.render_frame(cx);
            assert!(window.try_find(named("gallery-item", "missing")).is_none());
            assert!(
                window
                    .try_find(named("gallery-section", section.id))
                    .is_some(),
                "section {} did not render",
                section.id
            );
            for item in section.items {
                assert!(
                    window.try_find(named("gallery-item", item.name)).is_some(),
                    "item {} did not render",
                    item.name
                );
            }
        });
    }
}

#[gpui_kit::test]
fn copying_a_name_puts_the_exact_reference_on_the_clipboard(cx: &mut TestAppContext) {
    let (gallery, mut cx) = open_gallery(cx);
    let names = [
        "action::icon_button",
        "gpui_kit::component::spinner::Spinner",
    ];
    for name in names {
        cx.update(|window, cx| {
            gallery.update(cx, |gallery, cx| gallery.reveal(name, window, cx));
            window.render_frame(cx);
            window.click(named("gallery-copy", name), cx);
        });
        let copied = cx.read_from_clipboard().and_then(|item| item.text());
        assert_eq!(copied.as_deref(), Some(name));
    }
}

#[gpui_kit::test]
fn hiding_unused_kit_components_removes_their_cards(cx: &mut TestAppContext) {
    let (gallery, mut cx) = open_gallery(cx);
    let kit = SECTIONS
        .iter()
        .filter(|section| section.library == Library::Kit);
    for section in kit {
        for hide in [false, true] {
            cx.update(|window, cx| {
                gallery.update(cx, |gallery, cx| {
                    gallery.select_section(section.id, window, cx);
                    gallery.set_hide_unused(hide, cx);
                });
                window.render_frame(cx);
                for item in section.items {
                    let shown = window.try_find(named("gallery-item", item.name)).is_some();
                    assert_eq!(shown, !(hide && item.is_unused_kit()), "{}", item.name);
                }
            });
        }
    }
}

#[gpui_kit::test]
fn the_toolbar_toggle_hides_unused_components_and_reports_pressed(cx: &mut TestAppContext) {
    let (gallery, mut cx) = open_gallery(cx);
    cx.update(|window, cx| {
        let toolbar = gallery.update(cx, |gallery, cx| gallery.render_toolbar(window, cx));
        assert!(toolbar.is_some());
        assert!(!gallery.read(cx).hide_unused);
        gallery.update(cx, |gallery, cx| gallery.set_hide_unused(true, cx));
        assert!(gallery.read(cx).hide_unused);
        let status = gallery.read(cx).status(cx).unwrap_or_default();
        assert!(status.contains("GPUI Kit components used"), "{status}");
    });
}

#[gpui_kit::test]
fn item_search_finds_every_card_and_reveals_its_section(cx: &mut TestAppContext) {
    let (gallery, mut cx) = open_gallery(cx);
    cx.update(|window, cx| {
        let Some(search) = gallery.read(cx).search(cx) else {
            panic!("the gallery offers item search");
        };
        for item in items() {
            assert!(search.items.find(item.name).is_some(), "{}", item.name);
        }
        let Some(target) = items().find(|item| item.is_unused_kit()) else {
            return;
        };
        gallery.update(cx, |gallery, cx| {
            gallery.set_hide_unused(true, cx);
            gallery.open_search_result(target.name, window, cx);
        });
        window.render_frame(cx);
        assert!(
            window
                .try_find(named("gallery-item", target.name))
                .is_some()
        );
        assert!(!gallery.read(cx).hide_unused);
    });
}

#[gpui_kit::test]
fn the_filter_shows_matching_cards_from_every_section(cx: &mut TestAppContext) {
    let (gallery, mut cx) = open_gallery(cx);
    cx.update(|window, cx| {
        let filter = gallery.read(cx).filter.clone();
        filter.update(cx, |filter, cx| filter.set_value("spinner", window, cx));
        window.render_frame(cx);
        let browse = Browse {
            query: "spinner",
            hide_unused: false,
        };
        let (groups, _) = browse.matches();
        assert!(!groups.is_empty());
        for (_, items) in groups {
            for item in items {
                assert!(window.try_find(named("gallery-item", item.name)).is_some());
            }
        }
        assert!(
            window
                .try_find(named("gallery-item", "action::icon_button"))
                .is_none()
        );
    });
}

/// Each shell card has a surface entry, and each component it is built
/// from is a gallery card the card can show.
#[test]
fn shell_surfaces_link_to_gallery_cards() {
    let names: HashSet<_> = items().map(|item| item.name).collect();
    for item in crate::samples::shell::ITEMS {
        let Some(surface) = crate::samples::shell::surface(item.name) else {
            panic!("{} has no surface entry", item.name);
        };
        assert!(
            !surface.composes.is_empty(),
            "{} lists no components",
            item.name
        );
        for component in surface.composes {
            assert!(
                names.contains(component),
                "{} links to {component}",
                item.name
            );
        }
    }
    assert_eq!(
        crate::samples::shell::SURFACES.len(),
        crate::samples::shell::ITEMS.len()
    );
}

/// The Navigator card shows both ways a hidden title draws: no header at
/// all, and a strip-height header kept for a badge and an action. The tree
/// keeps the Space's name either way.
#[gpui_kit::test]
fn the_navigator_card_shows_both_hidden_title_states(cx: &mut TestAppContext) {
    let (gallery, mut cx) = open_gallery(cx);
    cx.update(|window, cx| {
        // An app that hides the title by default still shows it on the
        // card's first samples.
        subspace_dashboard::workspace::navigator_title::install_default(false, cx);
        gallery.update(cx, |gallery, cx| {
            gallery.select_section("navigation", window, cx)
        });
        window.render_frame(cx);
        let shown = window.within("gallery-sample:navigator-views");
        assert!(shown.try_find("workspace-navigator-title").is_some());
        let strip = subspace_dashboard::workspace::strip_height(window, cx);
        let plain = window.within("gallery-sample:navigator-title-hidden");
        assert!(plain.try_find("workspace-navigator-header").is_none());
        assert!(plain.try_find("workspace-navigator-title").is_none());
        assert_eq!(
            plain.find("workspace-navigator-body").label(),
            Some("Issues")
        );
        let kept = window.within("gallery-sample:navigator-title-hidden-header");
        assert!(kept.try_find("workspace-navigator-title").is_none());
        let header = kept.find("workspace-navigator-header").bounds();
        assert_eq!(header.size.height, strip);
        assert_eq!(
            kept.find("workspace-navigator-body").label(),
            Some("Issues")
        );
    });
}
