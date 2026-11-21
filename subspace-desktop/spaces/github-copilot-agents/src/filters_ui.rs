//! The three filters: the toolbar buttons, the menus behind them and what
//! they store.

use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{AnyElement, Context, IntoElement, Window, div};
use subspace_framework::dashboard::action::{Intent, action_button, menu_button};
use subspace_framework::dashboard::menu::{MenuContents, MenuEvent, MenuOption};
use subspace_framework::dashboard::overflow_toolbar::{OverflowUnit, PopupEntry};
use subspace_framework::dashboard::prefs;
use subspace_framework::lucide::IconName;

use crate::CopilotAgents;
use crate::filters::Filters;
use crate::keys::{GROUP_KEY, KIND_KEY, OpenSession, Picking, REPO_KEY};
use crate::model::{Kind, StateGroup};
use crate::ui::{ALL, button_id, menu_title, unset_label};

const UNMEASURED_WIDTH: f32 = 0.;

// qual:allow(coupling, oi) reason: "Filter menus mutate and persist the Copilot Agents entity's selection, then rebuild its rows."
impl CopilotAgents {
    pub(crate) fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.filters = Filters::default();
        self.store_filters(cx);
        self.rebuild(cx);
        cx.notify();
    }

    /// Applies the value a filter menu or sub-list returned.
    pub(crate) fn pick_filter(&mut self, picking: Picking, value: &str, cx: &mut Context<Self>) {
        let value = (value != ALL).then(|| value.to_owned());
        match picking {
            Picking::Kind => self.filters.kind = value.as_deref().and_then(Kind::from_id),
            Picking::Group => {
                self.filters.group = value.as_deref().and_then(StateGroup::from_id);
            }
            Picking::Repo => self.filters.repo = value,
        }
        self.store_filters(cx);
        self.rebuild(cx);
        cx.notify();
    }

    fn store_filters(&self, cx: &mut Context<Self>) {
        prefs::set(cx, KIND_KEY, self.filters.kind.map_or("", Kind::id));
        prefs::set(cx, GROUP_KEY, self.filters.group.map_or("", StateGroup::id));
        prefs::set(cx, REPO_KEY, self.filters.repo.clone().unwrap_or_default());
    }

    /// The options one filter offers, with the value in use checked.
    pub(crate) fn filter_options(&self, picking: Picking) -> Vec<(String, String, bool)> {
        let (label, values, current) = match picking {
            Picking::Kind => (
                "Any kind",
                Kind::ALL
                    .map(|kind| (kind.id().to_owned(), kind.label().to_owned()))
                    .to_vec(),
                self.filters.kind.map(|kind| kind.id().to_owned()),
            ),
            Picking::Group => (
                "Any state",
                StateGroup::ALL
                    .map(|group| (group.id().to_owned(), group.label().to_owned()))
                    .to_vec(),
                self.filters.group.map(|group| group.id().to_owned()),
            ),
            Picking::Repo => (
                "Any repository",
                crate::filters::repos(&self.all_rows)
                    .into_iter()
                    .map(|repo| (repo.clone(), repo))
                    .collect(),
                self.filters.repo.clone(),
            ),
        };
        let mut options = vec![(ALL.to_owned(), label.to_owned(), current.is_none())];
        options.extend(
            values
                .into_iter()
                .map(|(value, label)| (value.clone(), label, current.as_deref() == Some(&value))),
        );
        options
    }

    /// Opens a filter's menu from its toolbar button. The filters sit on
    /// the list's toolbar, which an opened task's tab replaces, so a key or
    /// command on an opened task shows the list first: the menu then hangs
    /// from a button that is drawn, and its pick shows in the list.
    pub(crate) fn open_filter_menu(
        &mut self,
        picking: Picking,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.item.showing().is_some() {
            self.show_list(cx);
        }
        self.open_filter_menu_at(picking, None, window, cx);
    }

    fn open_filter_menu_at(
        &mut self,
        picking: Picking,
        anchor: Option<gpui_kit::Entity<subspace_framework::dashboard::anchored::Anchor>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let options: Vec<MenuOption> = self
            .filter_options(picking)
            .into_iter()
            .map(|(value, label, checked)| MenuOption::new(value, label).checked(checked))
            .collect();
        let title = menu_title(picking);
        let anchor = anchor.unwrap_or_else(|| self.menu_anchors.anchor(button_id(picking), cx));
        let Some(menu) =
            self.menu_anchors
                .open(anchor, MenuContents::new(title, title, options), window, cx)
        else {
            return;
        };
        let subscription = cx.subscribe_in(&menu, window, move |this, _, event, _, cx| {
            this.menu = None;
            if let MenuEvent::Pick(value) = event {
                this.pick_filter(picking, value, cx);
            }
            cx.notify();
        });
        self.menu = Some((menu, picking, subscription));
        cx.notify();
    }

    // ---- Toolbar -----------------------------------------------------------------------

    fn filter_button(&self, picking: Picking, cx: &mut Context<Self>) -> AnyElement {
        let set = self.filter_label(picking);
        let unset = unset_label(picking, self.navigator_open);
        let button = menu_button(
            button_id(picking),
            filter_icon(picking),
            set.as_deref().unwrap_or(unset),
            set.is_some(),
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_filter_menu(picking, window, cx);
        }));
        self.menu_anchors.trigger(button_id(picking), button, cx)
    }

    /// The value a filter is set to, or `None` while it keeps everything.
    fn filter_label(&self, picking: Picking) -> Option<String> {
        match picking {
            Picking::Kind => self.filters.kind.map(|kind| kind.label().to_owned()),
            Picking::Group => self.filters.group.map(|group| group.label().to_owned()),
            Picking::Repo => self.filters.repo.clone(),
        }
    }

    fn clear_unit(entity: gpui_kit::WeakEntity<CopilotAgents>) -> OverflowUnit {
        let inline_entity = entity.clone();
        OverflowUnit::inline(
            "github-copilot-agents-clear",
            UNMEASURED_WIDTH,
            move |_, cx| {
                inline_entity
                    .update(cx, |_, cx| {
                        action_button(
                            "github-copilot-agents-clear",
                            Intent::Neutral,
                            Some(IconName::RotateCcw),
                            "Clear filters",
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.clear_filters(cx)))
                    })
                    .map_or_else(
                        |_| div().into_any_element(),
                        |element| element.into_any_element(),
                    )
            },
            vec![PopupEntry::new(move || {
                let entity = entity.clone();
                PopupMenuItem::new("Clear filters")
                    .icon(IconName::RotateCcw)
                    .on_click(move |_, _, cx| {
                        _ = entity.update(cx, |this, cx| this.clear_filters(cx));
                    })
            })],
        )
    }

    /// Browser (o) for the opened task, on the trailing side of its tab's
    /// toolbar. A row with no page on GitHub has none.
    pub(crate) fn browser_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let link = self.target().and_then(crate::browser::link)?;
        Some(
            action_button(
                "github-copilot-agents-browser",
                Intent::Neutral,
                Some(IconName::ExternalLink),
                "Browser (o)",
            )
            .tooltip(link.label)
            .on_click(
                cx.listener(|this, _, window, cx| this.open_session(&OpenSession, window, cx)),
            )
            .into_any_element(),
        )
    }

    fn toolbar_units(&self, entity: gpui_kit::WeakEntity<CopilotAgents>) -> Vec<OverflowUnit> {
        let mut units = vec![
            filter_unit(
                Picking::Kind,
                self.filter_label(Picking::Kind),
                entity.clone(),
            ),
            filter_unit(
                Picking::Group,
                self.filter_label(Picking::Group),
                entity.clone(),
            ),
            filter_unit(
                Picking::Repo,
                self.filter_label(Picking::Repo),
                entity.clone(),
            ),
        ];
        units.extend((!self.filters.is_empty()).then(|| Self::clear_unit(entity)));
        units
    }

    /// The filters, for the leading side of the pane's toolbar. They stay
    /// one row: in a narrow pane the last of them move to an overflow menu.
    pub(crate) fn toolbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let units = self.toolbar_units(cx.entity().downgrade());
        let toolbar = subspace_framework::dashboard::overflow_toolbar::OverflowToolbar::sync(
            &mut self.overflow_toolbar,
            units,
            cx,
        );
        self.menu_anchors.set_toolbar(&toolbar);
        toolbar.into_any_element()
    }
}

fn filter_unit(
    picking: Picking,
    selected: Option<String>,
    entity: gpui_kit::WeakEntity<CopilotAgents>,
) -> OverflowUnit {
    let label = selected
        .as_deref()
        .unwrap_or(menu_title(picking))
        .to_owned();
    let checked = selected.is_some();
    let popup_entity = entity.clone();
    let entries = vec![PopupEntry::anchored(move |anchor| {
        let entity = popup_entity.clone();
        PopupMenuItem::new(format!("{label}…"))
            .icon(filter_icon(picking))
            .checked(checked)
            .on_click(move |_, window, cx| {
                _ = entity.update(cx, |this, cx| {
                    this.open_filter_menu_at(picking, Some(anchor.host()), window, cx);
                });
            })
    })];
    let inline_entity = entity.clone();
    OverflowUnit::inline(
        button_id(picking),
        UNMEASURED_WIDTH,
        move |_, cx| {
            inline_entity
                .update(cx, |this, cx| this.filter_button(picking, cx))
                .map_or_else(
                    |_| div().into_any_element(),
                    |element| element.into_any_element(),
                )
        },
        entries,
    )
}

fn filter_icon(picking: Picking) -> IconName {
    match picking {
        Picking::Kind => IconName::Bot,
        Picking::Group => IconName::ListFilter,
        Picking::Repo => IconName::GitBranch,
    }
}
