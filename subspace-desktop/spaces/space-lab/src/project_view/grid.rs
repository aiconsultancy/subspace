use super::ProjectView;
use crate::project::{
    Component, ComponentKind, InputValue, LiveState, ResultUpdate, ResultValue, TableData,
};
use gpui_kit::{AppContext as _, Context, Entity, SharedString, Subscription, Window};
use subspace_framework::dashboard::native_grid::{GridCell, GridExport, GridSnapshot, NativeGrid};

mod export;

pub(super) struct GridControl {
    pub view: Entity<NativeGrid>,
    pub feedback: SharedString,
    result: ResultUpdate,
    session: String,
    _subscription: Subscription,
}

impl ProjectView {
    pub(super) fn sync_grids(
        &mut self,
        state: &LiveState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut tables = Vec::new();
        collect_tables(&state.definition().components, &mut tables);
        self.grids.retain(|id, _| {
            tables.iter().any(|component| &component.id == id) && state.results().contains_key(id)
        });
        for component in tables {
            self.sync_grid(component, state, window, cx);
        }
    }

    fn sync_grid(
        &mut self,
        component: &Component,
        state: &LiveState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(result) = state.results().get(&component.id) else {
            return;
        };
        if self
            .grids
            .get(&component.id)
            .is_some_and(|grid| grid.result == *result && grid.session == state.session_id())
        {
            return;
        }
        let ResultValue::Table { table } = &result.value else {
            return;
        };
        if let Some(grid) = self.grids.get_mut(&component.id) {
            let Ok(snapshot) = snapshot(table) else {
                return;
            };
            grid.view
                .update(cx, |grid, cx| grid.replace(snapshot, window, cx));
            grid.result = result.clone();
            grid.session = state.session_id().to_owned();
        } else {
            self.create_grid(component, state, window, cx);
        }
    }

    fn create_grid(
        &mut self,
        component: &Component,
        state: &LiveState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(result) = state.results().get(&component.id) else {
            return;
        };
        let ResultValue::Table { table } = &result.value else {
            return;
        };
        let Ok(source) = snapshot(table) else {
            return;
        };
        let id = format!("project-{}-{}", state.definition().id, component.id);
        let view = cx.new(|cx| NativeGrid::new(id, component.label.clone(), source, window, cx));
        let component_id = component.id.clone();
        let subscription = cx.subscribe(&view, move |this, _, event: &GridExport, cx| {
            this.export_grid(&component_id, event, cx);
        });
        self.grids.insert(
            component.id.clone(),
            GridControl {
                view,
                result: result.clone(),
                session: state.session_id().to_owned(),
                feedback: "".into(),
                _subscription: subscription,
            },
        );
    }
}

fn collect_tables<'a>(components: &'a [Component], tables: &mut Vec<&'a Component>) {
    tables.extend(
        components
            .iter()
            .filter(|component| component.kind == ComponentKind::Table),
    );
    for component in components {
        collect_tables(component.children(), tables);
    }
}

fn snapshot(table: &TableData) -> Result<GridSnapshot, &'static str> {
    GridSnapshot::new(
        table.columns.clone(),
        table
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| match cell {
                        InputValue::Number(value) => GridCell::number(*value),
                        InputValue::String(value) => GridCell::text(value.clone()),
                    })
                    .collect()
            })
            .collect(),
    )
}
