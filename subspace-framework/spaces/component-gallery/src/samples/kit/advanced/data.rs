//! GPUI Kit data views: the diff, calendar, carousel and command list, with
//! fixture data.

use gpui_kit::component::Disableable as _;
use gpui_kit::component::calendar::{Calendar, CalendarState};
use gpui_kit::component::carousel::{
    Carousel, CarouselContent, CarouselItem, CarouselNext, CarouselPrevious, CarouselState,
};
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::diff::{Diff, DiffFile, DiffState};
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Styled as _, Window,
    div, px,
};
use subspace_dashboard::Tokens;
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::{Sample, bounded, sample};

const WIDE: (f32, f32) = (520., 220.);
const NARROW: (f32, f32) = (300., 220.);
const SLIDE: (f32, f32) = (240., 120.);
const SLIDES: [&str; 3] = ["First slide", "Second slide", "Third slide"];
const PATCH: &str = "diff --git a/src/fixture.rs b/src/fixture.rs\n--- a/src/fixture.rs\n+++ b/src/fixture.rs\n@@ -1,4 +1,5 @@\n fn fixture() -> &'static str {\n-    \"old value\"\n+    // The gallery shows a fixture patch.\n+    \"new value\"\n }\n \n";

pub(crate) fn diff_state(cx: &mut App) -> Entity<DiffState> {
    let files = DiffFile::parse(PATCH).unwrap_or_default();
    cx.new(|cx| DiffState::new(files, cx))
}

pub(crate) fn diffs(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "One file, one hunk",
        bounded(WIDE, tokens, Diff::new(&gallery.retained.kit.advanced.diff)),
    )]
}

pub(crate) fn calendars(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Click a day",
        Calendar::new(&gallery.retained.kit.advanced.calendar),
    )]
}

fn slide(label: &'static str, tokens: &Tokens) -> impl IntoElement {
    div()
        .w(px(SLIDE.0))
        .h(px(SLIDE.1))
        .flex()
        .items_center()
        .justify_center()
        .rounded(tokens.surface_radius)
        .bg(tokens.select)
        .child(label)
}

pub(crate) fn carousels(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let state = &gallery.retained.kit.advanced.carousel;
    let content =
        SLIDES
            .iter()
            .enumerate()
            .fold(CarouselContent::new(state), |content, (at, label)| {
                content.child(
                    CarouselItem::new(("gallery-kit:carousel-item", at), at, state)
                        .child(slide(label, tokens)),
                )
            });
    vec![sample(
        "Previous and next",
        Carousel::new("gallery-kit:carousel", state)
            .w(px(SLIDE.0))
            .child(content)
            .child(CarouselPrevious::new(state))
            .child(CarouselNext::new(state)),
    )]
}

pub(crate) fn commands(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let command = Command::new(&gallery.retained.kit.advanced.command)
        .placeholder("Type a command…")
        .group(
            CommandGroup::new()
                .label("Fixtures")
                .item(CommandItem::new().label("New fixture").icon(IconName::Plus))
                .item(
                    CommandItem::new()
                        .label("Open fixture")
                        .icon(IconName::FolderOpen),
                )
                .item(
                    CommandItem::new()
                        .label("Delete fixture")
                        .icon(IconName::Trash)
                        .disabled(true),
                ),
        )
        .group(
            CommandGroup::new()
                .label("Gallery")
                .item(CommandItem::new().label("Copy a name").icon(IconName::Copy)),
        );
    vec![sample(
        "Filter as you type",
        bounded(NARROW, tokens, command),
    )]
}

pub(crate) fn calendar_state(window: &mut Window, cx: &mut App) -> Entity<CalendarState> {
    cx.new(|cx| CalendarState::new(window, cx))
}

pub(crate) fn carousel_state(cx: &mut App) -> Entity<CarouselState> {
    cx.new(|_| CarouselState::new(SLIDES.len()))
}

pub(crate) fn command_state(window: &mut Window, cx: &mut App) -> Entity<CommandState> {
    cx.new(|cx| CommandState::new(window, cx))
}
