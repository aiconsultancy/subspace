use super::{BAR_PADDING_PX, LogView, MenuKind, level_icon, opens_menu};
use crate::{
    Tokens,
    action::{Intent, action_button},
    fields::{self, InputRole},
    icon_button::Metrics,
    links,
    overflow_toolbar::{OverflowUnit, PopupEntry},
};
use gpui_kit::component::{Disableable as _, h_flex, menu::PopupMenuItem};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    TestSupportExt as _, WeakEntity, div, px,
};
use subspace_core::log::{self, Level};
use subspace_lucide::IconName;

// Positive logical-pixel estimate, replaced by OverflowToolbar's natural-width measurement.
const INITIAL_UNIT_WIDTH_PX: f32 = 1.;

#[derive(Clone, Copy)]
enum Action {
    Copy,
    CopyPath,
    Folder,
}

impl Action {
    fn presentation(self) -> (&'static str, &'static str, IconName) {
        // In Action's declaration order, shared by toolbar and overflow.
        [
            ("log-copy", "Copy", IconName::Copy),
            ("log-copy-path", "Copy log path", IconName::File),
            ("log-folder", "Open log folder", IconName::FolderOpen),
        ][self as usize]
    }
}

pub(super) fn render(view: &LogView, tokens: &Tokens, cx: &mut Context<LogView>) -> AnyElement {
    let units = units(view, cx.weak_entity());
    view.toolbar
        .update(cx, |toolbar, cx| toolbar.set_units(units, cx));
    h_flex()
        .id("log-toolbar")
        .test_support()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .gap(Metrics::current(cx).gap())
        .px(tokens.pad_x)
        .py(px(BAR_PADDING_PX))
        .bg(tokens.bg)
        .border_b_1()
        .border_color(tokens.hair)
        .child(
            div()
                .id("log-search")
                .test_support()
                .w_40()
                .min_w_0()
                .flex_shrink_1()
                .child(fields::input(
                    &view.input,
                    InputRole::Search,
                    "Filter the log",
                )),
        )
        .child(
            div()
                .id("log-actions")
                .test_support()
                .flex_1()
                .min_w(Metrics::current(cx).overflow_width())
                .child(view.toolbar.clone()),
        )
        .into_any_element()
}

fn units(view: &LogView, owner: WeakEntity<LogView>) -> Vec<OverflowUnit> {
    vec![
        levels_unit(view.filter.level, owner.clone()),
        menu_unit(
            MenuKind::Target,
            IconName::Funnel,
            opens_menu(view.filter.target.as_deref().unwrap_or("All targets")),
            owner.clone(),
        ),
        follow_unit(view.follow, owner.clone()),
        action_unit(Action::Copy, false, owner.clone()),
        action_unit(Action::CopyPath, view.log_path.is_none(), owner.clone()),
        action_unit(Action::Folder, false, owner.clone()),
        menu_unit(
            MenuKind::Capture,
            IconName::Settings2,
            opens_menu(&format!("Capture {}", log::level().as_str())),
            owner,
        ),
    ]
}

fn levels_unit(selected: Level, owner: WeakEntity<LogView>) -> OverflowUnit {
    let entries = Level::ALL
        .into_iter()
        .map(|level| {
            let owner = owner.clone();
            PopupEntry::new(move || {
                let owner = owner.clone();
                PopupMenuItem::new(format!("Level: {}", level.as_str()))
                    .icon(level_icon(level))
                    .checked(level == selected)
                    .on_click(move |_, _, cx| {
                        _ = owner.update(cx, |view, cx| view.set_level(level, cx));
                    })
            })
        })
        .collect();
    OverflowUnit::inline(
        "log-levels",
        INITIAL_UNIT_WIDTH_PX,
        move |_, cx| {
            owner
                .update(cx, |view, cx| view.level_tabs(cx).into_any_element())
                .unwrap_or_else(|_| div().into_any_element())
        },
        entries,
    )
}

fn follow_unit(follow: bool, owner: WeakEntity<LogView>) -> OverflowUnit {
    let popup_owner = owner.clone();
    OverflowUnit::inline(
        "log-follow",
        INITIAL_UNIT_WIDTH_PX,
        move |_, cx| {
            owner
                .update(cx, |view, cx| view.follow_control(cx).into_any_element())
                .unwrap_or_else(|_| div().into_any_element())
        },
        vec![PopupEntry::new(move || {
            let owner = popup_owner.clone();
            PopupMenuItem::new("Follow")
                .icon(IconName::ArrowDownToLine)
                .checked(follow)
                .on_click(move |_, _, cx| {
                    _ = owner.update(cx, |view, cx| view.set_follow(!follow, cx));
                })
        })],
    )
}

fn menu_unit(
    kind: MenuKind,
    icon: IconName,
    label: String,
    owner: WeakEntity<LogView>,
) -> OverflowUnit {
    let popup_owner = owner.clone();
    OverflowUnit::inline(
        kind.id(),
        INITIAL_UNIT_WIDTH_PX,
        move |_, cx| {
            owner
                .update(cx, |view, cx| {
                    view.menu_button(icon, label.clone(), kind, cx)
                        .into_any_element()
                })
                .unwrap_or_else(|_| div().into_any_element())
        },
        vec![PopupEntry::new(move || {
            let owner = popup_owner.clone();
            PopupMenuItem::new(format!("{}…", super::menu_frame(kind).0))
                .icon(icon)
                .on_click(move |_, window, cx| {
                    _ = owner.update(cx, |view, cx| view.open_menu(kind, window, cx));
                })
        })],
    )
}

fn action_unit(action: Action, disabled: bool, owner: WeakEntity<LogView>) -> OverflowUnit {
    let (id, label, icon) = action.presentation();
    let popup_owner = owner.clone();
    OverflowUnit::inline(
        id,
        INITIAL_UNIT_WIDTH_PX,
        move |_, _| {
            let owner = owner.clone();
            action_button(id, Intent::Neutral, Some(icon), label)
                .disabled(disabled)
                .on_click(move |_, _, cx| {
                    _ = owner.update(cx, |view, cx| view.activate_toolbar(action, cx));
                })
        },
        vec![PopupEntry::new(move || {
            let owner = popup_owner.clone();
            PopupMenuItem::new(label)
                .icon(icon)
                .disabled(disabled)
                .on_click(move |_, _, cx| {
                    _ = owner.update(cx, |view, cx| view.activate_toolbar(action, cx));
                })
        })],
    )
}

impl LogView {
    fn activate_toolbar(&mut self, action: Action, cx: &mut Context<Self>) {
        match action {
            Action::Copy => links::copy(self.text(), cx),
            Action::CopyPath => {
                if let Some(path) = &self.log_path {
                    links::copy(path.to_string_lossy().into_owned(), cx);
                }
            }
            Action::Folder => {
                if let Some(directory) = log::directory().or_else(log::default_directory) {
                    _ = links::reveal_directory(&directory);
                }
            }
        }
    }
}
