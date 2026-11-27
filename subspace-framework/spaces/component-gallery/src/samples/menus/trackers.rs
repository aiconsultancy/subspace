//! The Issues Parent menu interaction with local, paginated fixture results.

use gpui_kit::{Context, Entity, SharedString, Subscription, Window};
use subspace_dashboard::action;
use subspace_dashboard::menu::{Menu, MenuContents, MenuEvent, MenuOption};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::{Sample, sample as gallery_sample};

const ANCHOR: &str = "gallery-sample:tracker-parent";
const CONTINUE: &str = ":continue";
const PAGE_SIZE: usize = 2;
const TRACKERS: [(&str, &str); 5] = [
    ("DEMO-11", "Desktop access"),
    ("DEMO-12", "Desktop tabs"),
    ("DEMO-13", "Desktop themes"),
    ("DEMO-14", "Release plan"),
    ("DEMO-15", "Quickstart"),
];

#[derive(Default)]
pub(crate) struct Trackers {
    selected: SharedString,
    open: Option<TrackerSearch>,
}

struct TrackerSearch {
    menu: Entity<Menu>,
    query: SharedString,
    limit: usize,
    _events: Subscription,
}

fn options(selected: &str, query: &str, limit: usize) -> (Vec<MenuOption>, Option<&'static str>) {
    let query = query.trim().to_lowercase();
    let mut matches = TRACKERS.into_iter().filter(|(key, title)| {
        key.to_lowercase().contains(&query) || title.to_lowercase().contains(&query)
    });
    let mut rows = vec![MenuOption::new("", "No parent").checked(selected.is_empty())];
    rows.extend(matches.by_ref().take(limit).map(|(key, title)| {
        MenuOption::new(key, format!("{key} · {title} · Open")).checked(key == selected)
    }));
    if matches.next().is_some() {
        rows.push(MenuOption::new(CONTINUE, "Load more trackers"));
    }
    let note = (rows.len() == 1).then_some("No matching trackers");
    (rows, note)
}

fn open(gallery: &mut ComponentGallery, window: &mut Window, cx: &mut Context<ComponentGallery>) {
    let (rows, _) = options(&gallery.trackers.selected, "", PAGE_SIZE);
    let contents = MenuContents::new("Parent", "Find a tracker by key or title", rows)
        .owner_filtered(true)
        .keep_open(true);
    let anchor = gallery.menu_anchors.anchor(ANCHOR, cx);
    gallery.trackers.open = gallery
        .menu_anchors
        .open(anchor, contents, window, cx)
        .map(|menu| TrackerSearch {
            _events: cx.subscribe_in(&menu, window, handle_event),
            menu,
            query: SharedString::default(),
            limit: PAGE_SIZE,
        });
    cx.notify();
}

fn handle_event(
    gallery: &mut ComponentGallery,
    menu: &Entity<Menu>,
    event: &MenuEvent,
    window: &mut Window,
    cx: &mut Context<ComponentGallery>,
) {
    let state = &mut gallery.trackers;
    let Some(search) = state.open.as_mut().filter(|search| &search.menu == menu) else {
        return;
    };
    match event {
        MenuEvent::Query(query) => {
            search.query = query.clone();
            search.limit = PAGE_SIZE;
        }
        MenuEvent::Pick(value) if value.as_str() == CONTINUE => search.limit += PAGE_SIZE,
        MenuEvent::Pick(value) => {
            state.selected = value.clone();
            state.open = None;
            gallery.menu_anchors.dismiss(window, cx);
            cx.notify();
            return;
        }
        MenuEvent::Close => {
            state.open = None;
            cx.notify();
            return;
        }
    }
    let (rows, note) = options(&state.selected, &search.query, search.limit);
    menu.update(cx, |menu, cx| {
        menu.set_options(rows, cx);
        menu.set_note(note.map(Into::into), cx);
    });
}

pub(super) fn sample(gallery: &ComponentGallery, cx: &mut Context<ComponentGallery>) -> Sample {
    let selected = &gallery.trackers.selected;
    let label = if selected.is_empty() {
        "No parent"
    } else {
        selected.as_str()
    };
    let button = action::menu_button(
        ANCHOR,
        IconName::ListTree,
        label.to_owned(),
        !selected.is_empty(),
    )
    .accessibility_label(format!("Parent: {label}"))
    .on_click(cx.listener(|gallery, _, window, cx| open(gallery, window, cx)));
    gallery_sample(
        "Tracker parent picker · search and load more",
        gallery.menu_anchors.trigger(ANCHOR, button, cx),
    )
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::TestWindowExt as _;

    use super::ANCHOR;
    use crate::tests::open_gallery;

    #[gpui_kit::test]
    fn tracker_picker_loads_more_resets_on_search_and_can_clear(cx: &mut TestAppContext) {
        let (gallery, mut cx) = open_gallery(cx);
        cx.update(|window, cx| {
            gallery.update(cx, |gallery, cx| {
                gallery.reveal("menu::MenuAnchors", window, cx)
            });
            window.render_frame(cx);
            window.click(ANCHOR, cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.try_find(("menu-option", 4_usize)).is_none());
        });
        cx.simulate_keystrokes("4");
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.try_find(("menu-option", 5_usize)).is_some());
        });
        cx.simulate_input("desktop");
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.try_find(("menu-option", 4_usize)).is_none());
            window.click(("menu-option", 3_usize), cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.try_find(("menu-option", 4_usize)).is_none());
            window.click(("menu-option", 3_usize), cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert_eq!(gallery.read(cx).trackers.selected.as_str(), "DEMO-13");
            assert!(gallery.read(cx).trackers.open.is_none());
            window.render_frame(cx);
            window.click(ANCHOR, cx);
        });
        cx.run_until_parked();
        cx.simulate_keystrokes("1");
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert!(gallery.read(cx).trackers.selected.is_empty());
            assert!(gallery.read(cx).trackers.open.is_none());
        });
    }
}
