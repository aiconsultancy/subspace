//! GPUI Kit tables: the virtualised, sortable DataTable and the simple Table
//! parts, over the same fixture rows.

use gpui_kit::component::table::{
    Column, ColumnSort, DataTable, Table, TableBody, TableCell, TableDelegate, TableHead,
    TableHeader, TableRow, TableState,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, SharedString, Window,
};
use subspace_dashboard::Tokens;

use crate::ComponentGallery;
use crate::samples::{Sample, bounded, sample, sized};

const BOX: (f32, f32) = (460., 220.);
const TABLE_WIDTH: f32 = 460.;
const NAME_WIDTH: f32 = 180.;
const COLUMN_WIDTH: f32 = 120.;
const WIDTHS: [f32; 3] = [NAME_WIDTH, COLUMN_WIDTH, COLUMN_WIDTH];
const COLUMNS: [(&str, &str); 3] = [("name", "Name"), ("board", "Board"), ("count", "Count")];
/// Fixture rows: a name, the board it belongs to and a count.
const ROWS: [(&str, &str, u32); 8] = [
    ("Sample fixture", "github", 12),
    ("Another fixture", "actions", 7),
    ("Old fixture", "irc", 3),
    ("New fixture", "issues", 21),
    ("Large fixture", "board-lab", 40),
    ("Small fixture", "web", 1),
    ("Shared fixture", "terminal", 9),
    ("Last fixture", "component-gallery", 5),
];

/// The DataTable sample's delegate.
pub(crate) struct Fixtures {
    rows: Vec<(&'static str, &'static str, u32)>,
}

fn cell(row: &(&'static str, &'static str, u32), col_ix: usize) -> SharedString {
    match col_ix {
        0 => row.0.into(),
        1 => row.1.into(),
        _ => row.2.to_string().into(),
    }
}

fn fixture_column(col_ix: usize) -> Column {
    let (key, name) = COLUMNS.get(col_ix).copied().unwrap_or_default();
    let width = WIDTHS.get(col_ix).copied().unwrap_or(COLUMN_WIDTH);
    Column::new(key, name).width(gpui_kit::px(width)).sortable()
}

/// Sorts fixture rows by one column, in place.
fn sort(rows: &mut [(&'static str, &'static str, u32)], col_ix: usize, sort: ColumnSort) {
    match col_ix {
        0 => rows.sort_by_key(|row| row.0),
        1 => rows.sort_by_key(|row| row.1),
        _ => rows.sort_by_key(|row| row.2),
    }
    if matches!(sort, ColumnSort::Descending) {
        rows.reverse();
    }
}

impl TableDelegate for Fixtures {
    fn columns_count(&self, _: &App) -> usize {
        COLUMNS.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        fixture_column(col_ix)
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        order: ColumnSort,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        sort(&mut self.rows, col_ix, order);
        cx.notify();
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        self.rows
            .get(row_ix)
            .map(|row| cell(row, col_ix))
            .unwrap_or_default()
    }

    fn cell_label(&self, row_ix: usize, col_ix: usize, _: &App) -> Option<SharedString> {
        self.rows.get(row_ix).map(|row| cell(row, col_ix))
    }
}

pub(crate) fn table_state(window: &mut Window, cx: &mut App) -> Entity<TableState<Fixtures>> {
    let delegate = Fixtures {
        rows: ROWS.to_vec(),
    };
    cx.new(|cx| TableState::new(delegate, window, cx))
}

pub(crate) fn data_tables(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Click a header to sort",
        bounded(
            BOX,
            tokens,
            DataTable::new(&gallery.retained.kit.advanced.table),
        ),
    )]
}

pub(crate) fn tables(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let header = TableHeader::new()
        .child(TableRow::new().children(COLUMNS.map(|(_, name)| TableHead::new().child(name))));
    let body = TableBody::new().children(ROWS.iter().take(COLUMNS.len()).map(|row| {
        TableRow::new()
            .children((0..COLUMNS.len()).map(|col_ix| TableCell::new().child(cell(row, col_ix))))
    }));
    vec![sample(
        "Header and three rows",
        sized(TABLE_WIDTH, Table::new().child(header).child(body)),
    )]
}
