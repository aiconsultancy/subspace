//! GPUI Kit advanced components, in GPUI Kit's documentation order, with
//! Plot, NativeMenu and Table from its story gallery.

mod charts;
mod data;
mod dock;
mod menus;
mod tables;
#[cfg(test)]
mod tests;
mod views;

use std::rc::Rc;

use gpui_kit::component::calendar::CalendarState;
use gpui_kit::component::carousel::CarouselState;
use gpui_kit::component::command::CommandState;
use gpui_kit::component::diff::DiffState;
use gpui_kit::component::dock::DockArea;
use gpui_kit::component::grid_layout::GridLayoutState;
use gpui_kit::component::list::ListState;
use gpui_kit::component::table::TableState;
use gpui_kit::component::tree::TreeState;
use gpui_kit::{App, Entity, Pixels, Size, Window};

use super::kit_source;
use crate::catalog::{Item, Kit};

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "gpui_kit::component::diff::Diff",
        source: kit_source!("diff/mod.rs"),
        used_in: "Not used in Subspace; the GitHub board draws its own diff view.",
        samples: data::diffs,
        kit: Some(Kit {
            paths: &["diff::Diff", "diff::DiffState"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::calendar::Calendar",
        source: kit_source!("time/calendar.rs"),
        used_in: "Not used in Subspace yet.",
        samples: data::calendars,
        kit: Some(Kit {
            paths: &["calendar::Calendar", "calendar::CalendarState"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::carousel::Carousel",
        source: kit_source!("carousel/carousel.rs"),
        used_in: "Not used in Subspace yet.",
        samples: data::carousels,
        kit: Some(Kit {
            paths: &["carousel::Carousel"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::command::Command",
        source: kit_source!("command/command.rs"),
        used_in: "The shell's command palette.",
        samples: data::commands,
        kit: Some(Kit {
            paths: &["command::Command", "command::CommandState"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::chart::BarChart",
        source: kit_source!("chart/mod.rs"),
        used_in: "Not used directly: Subspace's chart::Chart draws agent charts on the lower-level plot shapes. Samples: bar, line, area, pie, radar, candlestick and sankey.",
        samples: charts::charts,
        kit: Some(Kit {
            paths: &[
                "chart::BarChart",
                "chart::LineChart",
                "chart::AreaChart",
                "chart::PieChart",
                "chart::RadarChart",
                "chart::CandlestickChart",
                "chart::SankeyChart",
            ],
            used: false,
            prefer: Some("chart::Chart"),
        }),
    },
    Item {
        name: "gpui_kit::component::plot::Plot",
        source: kit_source!("plot/mod.rs (gpui-base)"),
        used_in: "The plot shapes, axes and tooltip behind Subspace's chart::Chart.",
        samples: charts::plots,
        kit: Some(Kit {
            paths: &["plot::Plot", "plot::PlotElement", "plot::shape"],
            used: true,
            prefer: Some("chart::Chart"),
        }),
    },
    Item {
        name: "gpui_kit::component::list::List",
        source: kit_source!("list/list.rs"),
        used_in: "List rows (ListItem) in the shell palette and Settings navigation.",
        samples: views::lists,
        kit: Some(Kit {
            paths: &["list::List", "list::ListItem", "list::ListState"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::menu::PopupMenu",
        source: kit_source!("menu/popup_menu.rs"),
        used_in: "Every menu, hosted through anchored::dropdown_menu and anchored context menus.",
        samples: menus::menus,
        kit: Some(Kit {
            paths: &[
                "menu::PopupMenu",
                "menu::PopupMenuItem",
                "menu::ContextMenuExt",
            ],
            used: true,
            prefer: Some("anchored::dropdown_menu"),
        }),
    },
    Item {
        name: "gpui_kit::component::native_menu::NativeMenu",
        source: kit_source!("native_menu/mod.rs"),
        used_in: "Not used in Subspace yet.",
        samples: menus::native_menus,
        kit: Some(Kit {
            paths: &["native_menu::NativeMenu"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::setting::Settings",
        source: kit_source!("setting/mod.rs"),
        used_in: "Not used in Subspace: Settings is a shell composition of shared fields.",
        samples: views::settings,
        kit: Some(Kit {
            paths: &["setting::Settings", "setting::SettingPage"],
            used: false,
            prefer: Some("fields::form_field"),
        }),
    },
    Item {
        name: "gpui_kit::component::table::DataTable",
        source: kit_source!("table/data_table.rs"),
        used_in: "Not used in Subspace; result grids use the shared native_grid::NativeGrid.",
        samples: tables::data_tables,
        kit: Some(Kit {
            paths: &["table::DataTable", "table::TableState"],
            used: false,
            prefer: Some("native_grid::NativeGrid"),
        }),
    },
    Item {
        name: "gpui_kit::component::table::Table",
        source: kit_source!("table/table.rs"),
        used_in: "Not used in Subspace yet.",
        samples: tables::tables,
        kit: Some(Kit {
            paths: &["table::Table", "table::TableRow"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::dock::DockArea",
        source: kit_source!("dock/mod.rs"),
        used_in: "The Channels Space's workspace and the shell's Help dock.",
        samples: dock::docks,
        kit: Some(Kit {
            paths: &["dock::DockArea", "dock::DockSkin"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::grid_layout::GridLayoutState",
        source: kit_source!("grid_layout/mod.rs (gpui-base)"),
        used_in: "Not used in Subspace yet.",
        samples: views::grid_layouts,
        kit: Some(Kit {
            paths: &["grid_layout::GridLayoutState", "grid_layout::grid_layout"],
            used: false,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::tab::TabBar",
        source: kit_source!("tab/tab_bar.rs"),
        used_in: "Section and resource tabs through tabs::tab_bar, and the log level tabs.",
        samples: menus::tabs,
        kit: Some(Kit {
            paths: &["tab::TabBar", "tab::Tab"],
            used: true,
            prefer: Some("tabs::tab_bar"),
        }),
    },
    Item {
        name: "gpui_kit::component::tree::Tree",
        source: kit_source!("tree.rs"),
        used_in: "Settings navigation.",
        samples: views::trees,
        kit: Some(Kit {
            paths: &["tree::Tree", "tree::TreeState", "tree::tree"],
            used: true,
            prefer: None,
        }),
    },
    Item {
        name: "gpui_kit::component::v_virtual_list",
        source: kit_source!("virtual_list.rs (gpui-base)"),
        used_in: "Not used directly; the shared native grid uses GPUI's uniform_list.",
        samples: views::virtual_lists,
        kit: Some(Kit {
            paths: &["v_virtual_list", "h_virtual_list", "VirtualList"],
            used: false,
            prefer: None,
        }),
    },
];

/// Retained entities of the advanced samples.
pub(crate) struct State {
    pub(crate) diff: Entity<DiffState>,
    pub(crate) calendar: Entity<CalendarState>,
    pub(crate) carousel: Entity<CarouselState>,
    pub(crate) command: Entity<CommandState>,
    pub(crate) list: Entity<ListState<views::FruitList>>,
    pub(crate) table: Entity<TableState<tables::Fixtures>>,
    pub(crate) dock: Entity<DockArea>,
    pub(crate) grid: Entity<GridLayoutState>,
    pub(crate) tree: Entity<TreeState>,
    pub(crate) rows: Rc<Vec<Size<Pixels>>>,
}

impl State {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            diff: data::diff_state(cx),
            calendar: data::calendar_state(window, cx),
            carousel: data::carousel_state(cx),
            command: data::command_state(window, cx),
            list: views::list_state(window, cx),
            table: tables::table_state(window, cx),
            dock: dock::dock_area(window, cx),
            grid: views::grid_state(cx),
            tree: views::tree_state(cx),
            rows: views::row_sizes(),
        }
    }
}
