//! Native tree navigation generated from included board registrations.

mod model;
mod trail;

#[cfg(test)]
pub(in crate::settings) use model::last_app_item;

pub(in crate::settings) use trail::page_trail;

use std::collections::HashMap;

use gpui_kit::base::actions::{SelectDown, SelectUp};
use gpui_kit::component::list::ListItem;
use gpui_kit::component::tree::{TreeItem, TreeState, tree};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement as _, Role, ScrollStrategy, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, div, rems, svg,
};
use subspace_lucide::IconName;

use super::{Settings, SettingsPage};
use model::app_items;

const TREE_ROW_HEIGHT_REM: f32 = 2.;
const TREE_DEPTH_INDENT_REM: f32 = 0.875;
/// The least height, in rows, the Spaces heading and tree keep when the
/// dialog is too short for both trees.
const SPACES_MIN_ROWS: f32 = 3.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavIcon {
    Lucide(IconName),
    Svg(&'static str),
}

pub(in crate::settings) struct Navigation {
    pub(in crate::settings) state: Entity<TreeState>,
    pub(in crate::settings) board_state: Entity<TreeState>,
    pages: HashMap<String, SettingsPage>,
    icons: HashMap<String, NavIcon>,
}

impl Navigation {
    pub(in crate::settings) fn contains(&self, page: &SettingsPage) -> bool {
        self.pages.values().any(|value| value == page)
    }

    pub(in crate::settings) fn select_general(&self, cx: &mut App) {
        self.board_state
            .update(cx, |tree, cx| tree.set_selected_index(None, cx));
        self.state
            .update(cx, |tree, cx| tree.set_selected_index(Some(0), cx));
    }

    pub(in crate::settings) fn reconcile(
        &mut self,
        boards: &[crate::BoardRegistration],
        page: &SettingsPage,
        cx: &mut App,
    ) {
        let model = model::with_extensions(
            boards,
            crate::settings::extensions::registrations(cx),
            crate::terminal::registration(cx).is_some(),
            crate::account::shared(cx).is_some(),
        );
        let selected = model
            .pages
            .iter()
            .find(|(_, destination)| *destination == page)
            .map(|(id, _)| id.clone());
        self.board_state.update(cx, |tree, cx| {
            tree.set_items(model.board_items, cx);
            if let Some(id) = selected.as_ref().filter(|id| id.starts_with("board:")) {
                tree.set_selected_item(Some(&TreeItem::new(id.clone(), "")), cx);
            }
        });
        self.pages = model.pages;
        self.icons = model.icons;
    }
    pub(in crate::settings) fn new(
        boards: &[crate::registry::BoardRegistration],
        cx: &mut Context<Settings>,
    ) -> Self {
        let model = model::with_extensions(
            boards,
            crate::settings::extensions::registrations(cx),
            crate::terminal::registration(cx).is_some(),
            crate::account::shared(cx).is_some(),
        );
        let state = cx.new(|cx| {
            let mut state = TreeState::new(cx).items(model.app_items);
            state.set_selected_index(Some(0), cx);
            state
        });
        Self {
            state,
            board_state: cx.new(|cx| TreeState::new(cx).items(model.board_items)),
            pages: model.pages,
            icons: model.icons,
        }
    }

    pub(in crate::settings) fn selected_page(&self, cx: &App) -> Option<SettingsPage> {
        self.states().find_map(|state| {
            let id = &state.read(cx).selected_item()?.id;
            self.pages.get(id.as_str()).cloned()
        })
    }

    pub(in crate::settings) fn states(&self) -> impl Iterator<Item = &Entity<TreeState>> {
        [&self.state, &self.board_state].into_iter()
    }

    pub(in crate::settings) fn activate(
        &self,
        selected: &Entity<TreeState>,
        cx: &mut App,
    ) -> Option<SettingsPage> {
        selected.read(cx).selected_item()?;
        for state in self.states().filter(|state| *state != selected) {
            if state.read(cx).selected_index().is_some() {
                state.update(cx, |tree, cx| tree.set_selected_index(None, cx));
            }
        }
        self.selected_page(cx)
    }

    /// Only cross-group arrows are intercepted. Each native tree retains its
    /// own expansion, selection and scroll-to-item behavior within the group.
    fn cross_boundary(&self, down: bool, window: &mut Window, cx: &mut App) {
        let (source, target) = if self.state.read(cx).selected_index().is_some() {
            (&self.state, &self.board_state)
        } else {
            (&self.board_state, &self.state)
        };
        let Some(index) = source.read(cx).selected_index() else {
            return;
        };
        let at_boundary = if down {
            source.read(cx).entry(index + 1).is_none()
        } else {
            index == 0
        };
        if !at_boundary || target.read(cx).entry(0).is_none() {
            return;
        }
        let index = if down { 0 } else { last_entry(target.read(cx)) };
        source.update(cx, |tree, cx| tree.set_selected_index(None, cx));
        target.update(cx, |tree, cx| {
            tree.set_selected_index(Some(index), cx);
            tree.scroll_to_item(index, ScrollStrategy::Top);
            tree.focus(window, cx);
        });
        cx.stop_propagation();
    }
}

impl Settings {
    /// Select a board before the freshly opened tree has been expanded.
    pub(crate) fn select_initial_board(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(board) = self.boards.iter().find(|board| board.id == id) {
            self.page = SettingsPage::Board(board.id);
            self.navigation.state.update(cx, |tree, cx| {
                tree.set_selected_index(None, cx);
            });
            self.navigation.board_state.update(cx, |tree, cx| {
                tree.set_selected_item(Some(&TreeItem::new(format!("board:{id}"), "")), cx);
            });
            cx.notify();
        }
    }

    #[cfg(test)]
    pub(crate) fn shows_board_page(&self, id: &str) -> bool {
        matches!(self.page, SettingsPage::Board(board) if board == id)
    }

    #[cfg(test)]
    pub(crate) fn shows_developer_page(&self) -> bool {
        self.page == SettingsPage::Developer
    }

    /// Select the Developer page, as Developer settings… opens Settings.
    #[cfg(debug_assertions)]
    pub(crate) fn select_developer_page(&mut self, cx: &mut Context<Self>) {
        self.page = SettingsPage::Developer;
        self.navigation.board_state.update(cx, |tree, cx| {
            tree.set_selected_index(None, cx);
        });
        self.navigation.state.update(cx, |tree, cx| {
            tree.set_selected_item(Some(&TreeItem::new(model::DEVELOPER_ITEM, "")), cx);
        });
        cx.notify();
    }

    pub(super) fn navigation_tree(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = subspace_dashboard::Tokens::current(cx);
        v_flex()
            .size_full()
            .min_h_0()
            .capture_action(cx.listener(|settings, _: &SelectDown, window, cx| {
                settings.navigation.cross_boundary(true, window, cx);
            }))
            .capture_action(cx.listener(|settings, _: &SelectUp, window, cx| {
                settings.navigation.cross_boundary(false, window, cx);
            }))
            .child(group_heading(
                "settings-app-heading",
                "Application",
                &tokens,
            ))
            // Every Application row shows: the tree asks for its full height
            // and, as elements do by default, shrinks (then scrolls) only once
            // the Spaces below are down to their minimum. An equal split hid
            // an extension's row.
            .child(
                navigation_tree(&self.navigation.state, self.navigation.icons.clone())
                    .min_h_0()
                    .h(rems(
                        (app_rows(
                            crate::account::shared(cx).is_some(),
                            self.terminal.is_some(),
                        ) + self.extensions.len()) as f32
                            * TREE_ROW_HEIGHT_REM,
                    )),
            )
            .children((!self.boards.is_empty()).then(|| {
                v_flex()
                    .flex_1()
                    .min_h(rems(SPACES_MIN_ROWS * TREE_ROW_HEIGHT_REM))
                    .pt_3()
                    .child(group_heading("settings-boards-heading", "Spaces", &tokens))
                    .child(
                        navigation_tree(
                            &self.navigation.board_state,
                            self.navigation.icons.clone(),
                        )
                        .flex_1()
                        .min_h_0(),
                    )
            }))
            .into_any_element()
    }
}

/// The Application pages the tree lists before any extension.
pub(in crate::settings) fn app_rows(account: bool, terminal: bool) -> usize {
    app_items(account).len() + usize::from(terminal)
}

fn group_heading(
    id: &'static str,
    label: &'static str,
    tokens: &subspace_dashboard::Tokens,
) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Heading)
        .aria_label(label)
        .test_support()
        // An inert heading must not move focus out of the native tree.
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .flex_shrink_0()
        .min_w_0()
        .whitespace_nowrap()
        .text_ellipsis()
        .px_2()
        .pb_2()
        .text_sm()
        .text_color(tokens.muted)
        .child(label)
}

fn navigation_tree(
    state: &Entity<TreeState>,
    icons: HashMap<String, NavIcon>,
) -> gpui_kit::component::tree::Tree {
    tree(state, move |_, entry, _, _, cx| {
        let id = entry.item().id.to_string();
        let icon = icons.get(&id).copied();
        ListItem::new(format!("settings-{id}"))
            .accessibility_label(entry.item().label.clone())
            .h(rems(TREE_ROW_HEIGHT_REM))
            .pl(rems(entry.depth() as f32 * TREE_DEPTH_INDENT_REM))
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div()
                            .w_4()
                            .flex_shrink_0()
                            .children(entry.is_folder().then(|| {
                                if entry.is_expanded() {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                }
                            })),
                    )
                    .children(icon.map(|icon| navigation_icon(icon, cx)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .child(entry.item().label.clone()),
                    ),
            )
    })
    .size_full()
}

fn last_entry(state: &TreeState) -> usize {
    let mut index = 0;
    while state.entry(index + 1).is_some() {
        index += 1;
    }
    index
}

fn navigation_icon(icon: NavIcon, cx: &App) -> AnyElement {
    match icon {
        NavIcon::Lucide(icon) => icon.into_any_element(),
        NavIcon::Svg(path) => svg()
            .path(path)
            .size_4()
            .flex_shrink_0()
            .text_color(subspace_dashboard::Tokens::current(cx).fg_strong)
            .into_any_element(),
    }
}
