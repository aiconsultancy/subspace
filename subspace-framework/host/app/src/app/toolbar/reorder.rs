//! Drag the horizontal Space tabs and persist their order.

use gpui_kit::component::button::Button;
use gpui_kit::{
    App, AppContext as _, Context, Div, InteractiveElement as _, Stateful,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window,
};
use std::sync::Arc;
use subspace_core::settings::{SettingsError, SettingsFile};
use subspace_dashboard::{Services, Toast, Tokens, glass};

use super::{SubspaceApp, TabData};
use crate::settings::board_order::{BoardDrag, DragPreview, move_to};

pub(super) fn draggable_tab(
    button: Button,
    data: &TabData,
    tokens: Tokens,
    app: WeakEntity<SubspaceApp>,
) -> Stateful<Div> {
    let id = data.id;
    let owner = app.entity_id();
    glass::bleed(button)
        .id(format!("space-drag-tab-{id}"))
        .on_drag(
            BoardDrag {
                owner,
                id,
                title: data.title.clone(),
            },
            |drag, _, _, cx| cx.new(|_| DragPreview(drag.title.clone())),
        )
        .drag_over::<BoardDrag>(move |style, drag, _, _| {
            if drag.owner == owner && drag.id != id {
                return style.bg(tokens.select);
            }
            style
        })
        .on_drop(move |drag: &BoardDrag, window, cx| {
            _ = app.update(cx, |app, cx| app.move_space_tab(drag, id, window, cx));
        })
}

impl SubspaceApp {
    fn move_space_tab(
        &mut self,
        drag: &BoardDrag,
        target: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if drag.owner != cx.entity_id() || drag.id == target {
            return;
        }
        let result = save_order(drag.id, target, cx);
        match result {
            Ok(Some(order)) => {
                Arc::make_mut(&mut cx.global_mut::<Services>().config).board_order = order;
                self.sync_navigation_preferences(window, cx);
            }
            Err(error) => self.push_toast(
                Toast::error(format!("Couldn't save Space order: {error}")),
                cx,
            ),
            Ok(None) => {}
        }
    }
}

fn save_order(source: &str, target: &str, cx: &App) -> Result<Option<Vec<String>>, SettingsError> {
    let Some(services) = cx.try_global::<Services>() else {
        return Ok(None);
    };
    let mut file = SettingsFile::open(&services.config_file)?;
    let included = crate::registry::included_registrations(cx);
    let mut order = crate::registry::normalize_order(&included, &file.values().board_order);
    let Some((source, target)) = positions(&order, source, target) else {
        return Ok(None);
    };
    move_to(&mut order, source, target);
    file.save_board_order(&order)?;
    Ok(Some(order))
}

fn positions(order: &[String], source: &str, target: &str) -> Option<(usize, usize)> {
    let source = order.iter().position(|id| id == source)?;
    let target = order.iter().position(|id| id == target)?;
    Some((source, target))
}
