//! GPUI Kit list views: the list, settings page, tree, virtual list and
//! grid layout, with fixture data.

use std::rc::Rc;

use gpui_kit::component::grid_layout::{GridArea, GridItem, GridLayoutState, grid_layout};
use gpui_kit::component::list::{List, ListDelegate, ListItem, ListState};
use gpui_kit::component::setting::{
    SettingField, SettingGroup, SettingItem, SettingPage, Settings,
};
use gpui_kit::component::tree::{TreeItem, TreeState, tree};
use gpui_kit::component::{IndexPath, h_flex, v_virtual_list};
use gpui_kit::{
    App, AppContext as _, Context, Entity, ParentElement as _, Pixels, Size, Styled as _, Window,
    div, px, size,
};
use subspace_dashboard::Tokens;
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::{Sample, Values, bounded, flag, sample, store_flag};

const WIDE: (f32, f32) = (520., 220.);
const NARROW: (f32, f32) = (300., 220.);
const ROW_WIDTH: f32 = 280.;
const ROW_HEIGHT: f32 = 28.;
const VIRTUAL_ROWS: usize = 1000;
const GRID_COLUMNS: usize = 6;
/// The revenue tile's width and both top tiles' height, in grid cells.
const WIDE_SPAN: usize = 4;
const TALL_SPAN: usize = 2;
const FRUIT: [&str; 6] = ["Apple", "Banana", "Cherry", "Date", "Elderberry", "Fig"];
const NOTIFY: &str = "kit-settings-notify";
const COMPACT: &str = "kit-settings-compact";

/// The list samples' delegate: fixture fruit with one selected row.
pub(crate) struct FruitList {
    selected: Option<IndexPath>,
}

impl ListDelegate for FruitList {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        FRUIT.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let name = FRUIT.get(ix.row)?;
        Some(
            ListItem::new(("gallery-kit:list-item", ix.row))
                .selected(self.selected == Some(ix))
                .child(*name),
        )
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected = ix;
        cx.notify();
    }
}

pub(crate) fn list_state(window: &mut Window, cx: &mut App) -> Entity<ListState<FruitList>> {
    cx.new(|cx| ListState::new(FruitList { selected: None }, window, cx).searchable(true))
}

pub(crate) fn tree_state(cx: &mut App) -> Entity<TreeState> {
    let items = vec![
        TreeItem::new("src", "src").expanded(true).children(vec![
            TreeItem::new("src/lib.rs", "lib.rs"),
            TreeItem::new("src/view.rs", "view.rs"),
        ]),
        TreeItem::new("docs", "docs").children(vec![TreeItem::new("docs/README.md", "README.md")]),
        TreeItem::new("Cargo.toml", "Cargo.toml"),
    ];
    cx.new(|cx| TreeState::new(cx).items(items))
}

pub(crate) fn grid_state(cx: &mut App) -> Entity<GridLayoutState> {
    let areas = [
        GridArea::new("revenue")
            .col_span(WIDE_SPAN)
            .row_span(TALL_SPAN),
        GridArea::new("orders")
            .col(WIDE_SPAN)
            .col_span(GRID_COLUMNS - WIDE_SPAN)
            .row_span(TALL_SPAN),
        GridArea::new("notes")
            .row(TALL_SPAN)
            .col_span(GRID_COLUMNS)
            .row_span(1),
    ];
    cx.new(|cx| {
        GridLayoutState::new(cx)
            .cols(GRID_COLUMNS)
            .default_areas(areas)
    })
}

pub(crate) fn row_sizes() -> Rc<Vec<Size<Pixels>>> {
    Rc::new(vec![size(px(ROW_WIDTH), px(ROW_HEIGHT)); VIRTUAL_ROWS])
}

pub(crate) fn lists(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Searchable; click or use the arrow keys",
        bounded(
            NARROW,
            tokens,
            List::new(&gallery.retained.kit.advanced.list),
        ),
    )]
}

fn setting_switch(values: &Entity<Values>, key: &'static str) -> SettingField<bool> {
    let read = values.clone();
    let write = values.clone();
    SettingField::switch(
        move |cx: &App| read.read(cx).flag(key, false),
        move |on: bool, cx: &mut App| store_flag(&write, key, on, cx),
    )
}

pub(crate) fn settings(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let values = &gallery.retained.values;
    let compact = flag(values, COMPACT, false, cx);
    let page = SettingPage::new("General").default_open(true).groups(vec![
        SettingGroup::new().title("Fixture").items(vec![
            SettingItem::new("Notifications", setting_switch(values, NOTIFY))
                .description("Show a notice when a fixture changes."),
            SettingItem::new("Compact rows", setting_switch(values, COMPACT)).description(
                if compact {
                    "Rows are compact."
                } else {
                    "Rows are comfortable."
                },
            ),
        ]),
    ]);
    vec![sample(
        "One page, one group",
        bounded(
            WIDE,
            tokens,
            Settings::new("gallery-kit:settings").pages(vec![page]),
        ),
    )]
}

pub(crate) fn trees(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let tree = tree(
        &gallery.retained.kit.advanced.tree,
        |ix, entry, selected, _, _| {
            let icon = if entry.is_folder() {
                IconName::Folder
            } else {
                IconName::File
            };
            ListItem::new(("gallery-kit:tree-item", ix))
                .selected(selected)
                .pl(px(ROW_HEIGHT) * entry.depth())
                .child(
                    h_flex()
                        .gap_2()
                        .child(icon)
                        .child(entry.item().label.clone()),
                )
        },
    );
    vec![sample(
        "Click a folder to expand it",
        bounded(NARROW, tokens, tree),
    )]
}

pub(crate) fn virtual_lists(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let sizes = gallery.retained.kit.advanced.rows.clone();
    let list = v_virtual_list(
        cx.entity(),
        "gallery-kit:virtual-list",
        sizes,
        |_, range, _, _| {
            range
                .map(|row| {
                    div()
                        .h(px(ROW_HEIGHT))
                        .px_2()
                        .child(format!("Fixture row {row}"))
                })
                .collect::<Vec<_>>()
        },
    );
    vec![sample(
        "A thousand rows; only visible ones render",
        bounded(NARROW, tokens, list.size_full()),
    )]
}

pub(crate) fn grid_layouts(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let tile = |id: &'static str, title: &'static str| {
        GridItem::new(id).child(
            div()
                .size_full()
                .p_2()
                .rounded(tokens.surface_radius)
                .bg(tokens.select)
                .child(title),
        )
    };
    let grid = grid_layout(
        "gallery-kit:grid-layout",
        &gallery.retained.kit.advanced.grid,
        cx,
    )
    .items([
        tile("revenue", "Revenue"),
        tile("orders", "Orders"),
        tile("notes", "Notes"),
    ]);
    vec![sample("Drag or resize a tile", bounded(WIDE, tokens, grid))]
}
