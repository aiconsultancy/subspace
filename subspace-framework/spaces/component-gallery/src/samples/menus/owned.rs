//! Menus with an owner: the popover anchor, a row's context menu and the
//! repository picker, which searches a fixed fixture list off the UI thread.

use std::sync::Arc;

use gpui_kit::{
    Context, Entity, InteractiveElement as _, ParentElement as _, Styled as _, Subscription,
    Window, div,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{self, Intent};
use subspace_dashboard::anchored::AnchoredContextMenuExt as _;
use subspace_dashboard::repositories::{
    self, PickerSetup, RepositoryPicker, RepositoryPickerEvent,
};
use subspace_dashboard::row_menu::{self, RowMenu};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::{FIXTURE_URL, Sample, sample};

/// The named anchor of the repository picker sample.
pub(crate) const REPOSITORY_ANCHOR: &str = "gallery-sample:repositories";
const ROW_WIDTH: f32 = 320.;
const REPOSITORIES: [&str; 5] = [
    "example/component-gallery",
    "example/fixtures",
    "example/sample-app",
    "example/docs",
    "example/tools",
];

/// The open picker and the fixture favourites it edits.
#[derive(Default)]
pub(crate) struct Repositories {
    favourites: Vec<String>,
    filter: bool,
    open: Option<(Entity<RepositoryPicker>, Subscription)>,
}

fn search(text: &str) -> Result<Vec<String>, String> {
    let text = text.to_lowercase();
    Ok(REPOSITORIES
        .iter()
        .filter(|repo| repo.contains(&text))
        .map(|repo| (*repo).to_owned())
        .collect())
}

/// Opens the fixture repository picker, or closes it when it is open.
fn open(gallery: &mut ComponentGallery, window: &mut Window, cx: &mut Context<ComponentGallery>) {
    let anchor = gallery.menu_anchors.anchor(REPOSITORY_ANCHOR, cx);
    let setup = PickerSetup {
        favourites: gallery.repositories.favourites.clone(),
        filter: Some(gallery.repositories.filter),
        search: Arc::new(search),
    };
    let picker = RepositoryPicker::open(&gallery.menu_anchors, anchor, setup, window, cx);
    gallery.repositories.open = picker.map(|picker| {
        let subscription = cx.subscribe(&picker, |gallery, picker, event, cx| {
            picked(gallery, &picker, event, cx);
        });
        (picker, subscription)
    });
}

/// Applies a pick to the fixture favourites, as a board saves its own.
fn picked(
    gallery: &mut ComponentGallery,
    picker: &Entity<RepositoryPicker>,
    event: &RepositoryPickerEvent,
    cx: &mut Context<ComponentGallery>,
) {
    let state = &mut gallery.repositories;
    match event {
        RepositoryPickerEvent::Toggle(repo) => {
            state.favourites = repositories::toggled(&state.favourites, repo);
        }
        RepositoryPickerEvent::ToggleFilter => state.filter = !state.filter,
        RepositoryPickerEvent::Closed => {
            state.open = None;
            return;
        }
    }
    let (favourites, filter) = (state.favourites.clone(), Some(state.filter));
    picker.update(cx, |picker, cx| {
        picker.set_favourites(favourites, filter, cx)
    });
}

pub(crate) fn anchors(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let button = action::action_button(
        "gallery-sample:anchor",
        Intent::Neutral,
        Some(IconName::MessageSquare),
        "Show popover",
    );
    vec![sample(
        "Retained popover body; Escape closes it",
        gallery.retained.popover.trigger(button),
    )]
}

pub(crate) fn row_menus(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let facts = RowMenu::new("Sample fixture")
        .id("#42")
        .link("GitHub", FIXTURE_URL)
        .opens_details(true);
    let row = div()
        .id("gallery-sample:row-menu")
        .w(gpui_kit::px(ROW_WIDTH))
        .px(tokens.pad_x)
        .py_1()
        .rounded(tokens.control_radius)
        .bg(tokens.select)
        .child("#42 Sample fixture · right-click")
        .anchored_context_menu(move |menu, _, _| {
            row_menu::popup(menu, row_menu::row_menu_items(&facts), None)
        });
    vec![sample("A row's standard context menu", row)]
}

pub(crate) fn repository_pickers(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let count = gallery.repositories.favourites.len();
    let button = action::menu_button(
        "gallery-sample:repositories-button",
        IconName::BookMarked,
        format!("Repositories ({count} favourite)"),
        count > 0,
    )
    .on_click(cx.listener(|this, _, window, cx| open(this, window, cx)));
    vec![sample(
        "Fixture search; star to favourite",
        gallery.menu_anchors.trigger(REPOSITORY_ANCHOR, button, cx),
    )]
}
