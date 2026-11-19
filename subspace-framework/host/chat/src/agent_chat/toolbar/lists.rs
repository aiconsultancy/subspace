//! The searchable model list: a GPUI Kit `Command` palette in a popover, so
//! `/model` can open it without a click.

use gpui_kit::component::ThemeStyled as _;
use gpui_kit::component::command::CommandState;
use gpui_kit::component::command::{Command, CommandGroup, CommandItem};
use gpui_kit::{
    App, AppContext as _, Context, Div, Entity, FocusHandle, Focusable, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, TestSupportExt as _, WeakEntity, Window, div, px,
};
use subspace_lucide::IconName;

/// Wide enough for full model names and session titles.
const LIST_WIDTH: f32 = 320.;

use super::labels::{current_model, model_choice, model_label, model_sections};
use super::menus::button;
use super::{ChatToolbar, ModelOption};
use crate::action::{Intent, icon_button};
use crate::anchored;
use crate::tokens::Tokens;
use crate::truncated;

/// One section of the model list. Every row carries a star that toggles the
/// model's favourite state without choosing it.
#[derive(Clone, Copy)]
struct ModelGroup<'a> {
    heading: &'static str,
    models: &'a [ModelOption],
    starred: bool,
    chosen: Option<&'a str>,
    toolbar: &'a WeakEntity<ChatToolbar>,
}

fn model_group(group: ModelGroup) -> CommandGroup {
    let items: Vec<CommandItem> = group
        .models
        .iter()
        .map(|model| {
            let row = (model.clone(), group.starred, group.toolbar.clone());
            CommandItem::new()
                .label(model.label.clone())
                .keywords([model.id.clone()])
                .checked(Some(model.id.as_ref()) == group.chosen)
                .child(move |_, cx| model_row(&row.0, row.1, &row.2, cx))
        })
        .collect();
    CommandGroup::new().label(group.heading).items(items)
}

/// The star's accessible name and colour: yellow once starred.
fn star_style(starred: bool, tokens: &Tokens) -> (&'static str, Hsla) {
    if starred {
        ("Remove from favourites", tokens.yellow)
    } else {
        ("Add to favourites", tokens.muted)
    }
}

fn model_row(
    model: &ModelOption,
    starred: bool,
    toolbar: &WeakEntity<ChatToolbar>,
    cx: &App,
) -> Div {
    let (name, colour) = star_style(starred, &Tokens::current(cx));
    let toolbar = toolbar.clone();
    let id = model.id.clone();
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .items_center()
        .child(
            truncated::text(
                SharedString::from(format!("chat-model-name:{id}")),
                model.label.clone(),
            )
            .flex_1(),
        )
        .child(
            icon_button(
                SharedString::from(format!("chat-model-star-{id}")),
                Intent::Quiet,
                IconName::Star,
                name,
            )
            .compact()
            .text_color(colour)
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                let _ = toolbar.update(cx, |toolbar, cx| toolbar.toggle_favourite(&id, cx));
            }),
        )
}

struct ModelList {
    toolbar: WeakEntity<ChatToolbar>,
    search: Entity<CommandState>,
    _subscription: Subscription,
}

impl Focusable for ModelList {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.search.focus_handle(cx)
    }
}

impl Render for ModelList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("chat-model-body")
            .test_support()
            .popover_style(cx)
            .p_3()
            .children(self.toolbar.upgrade().map(|toolbar| {
                toolbar
                    .read(cx)
                    .model_command(&self.search, toolbar.downgrade())
            }))
    }
}

impl ChatToolbar {
    pub(super) fn model_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let label = model_label(current_model(
            &self.model.models,
            self.model.model.as_deref(),
        ));
        self.anchors.trigger(
            "chat-model",
            button("chat-model", IconName::Cpu, "Model")
                .label(label)
                .on_click(cx.listener(|toolbar, _, window, cx| toolbar.toggle_models(window, cx))),
            cx,
        )
    }

    fn toggle_models(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let host = self.anchors.anchor("chat-model", cx);
        if host.read(cx).is_open() {
            host.update(cx, |anchor, cx| anchor.dismiss(window, cx));
        } else {
            self.open_models(window, cx);
        }
    }

    /// Open the model list, as `/model` does.
    pub fn open_models(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_popups(window, cx);
        let host = self.anchors.anchor("chat-model", cx);
        self.show_models(host, window, cx);
    }

    pub(super) fn show_models(
        &self,
        host: Entity<anchored::Anchor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let toolbar = cx.entity();
        let search = self.model_search.clone();
        let body = cx.new(|cx| ModelList {
            toolbar: toolbar.downgrade(),
            search,
            _subscription: cx.observe(&toolbar, |_, _, cx| cx.notify()),
        });
        host.update(cx, |anchor, cx| anchor.show(body, window, cx));
    }

    fn model_command(&self, search: &Entity<CommandState>, toolbar: WeakEntity<Self>) -> Command {
        let close = toolbar.clone();
        let chosen = self.model.model.clone();
        let [starred, rest] = model_sections(&self.model.models, &self.favourites);
        let sections = [starred.clone(), rest.clone()];
        let favourites = ModelGroup {
            heading: "Favourites",
            models: &starred,
            starred: true,
            chosen: chosen.as_deref(),
            toolbar: &toolbar,
        };
        Command::new(search)
            .w(px(LIST_WIDTH))
            .placeholder("Search models")
            .bordered(false)
            .group(model_group(favourites))
            .group(model_group(ModelGroup {
                heading: "Models",
                models: &rest,
                starred: false,
                ..favourites
            }))
            .on_confirm(move |path, window, cx| {
                let Some(event) = model_choice(&sections, path.section, path.row) else {
                    return;
                };
                _ = toolbar.update(cx, |toolbar, cx| {
                    toolbar.dismiss_popups(window, cx);
                    cx.emit(event);
                });
            })
            .on_cancel(move |window, cx| {
                _ = close.update(cx, |toolbar, cx| toolbar.dismiss_popups(window, cx));
            })
    }
}

#[cfg(test)]
mod tests {
    use super::star_style;
    use crate::tokens::Tokens;

    #[test]
    fn a_starred_model_shows_a_yellow_star_that_unstars() {
        let tokens = Tokens::one_dark();
        assert_eq!(
            star_style(true, &tokens),
            ("Remove from favourites", tokens.yellow)
        );
        assert_eq!(
            star_style(false, &tokens),
            ("Add to favourites", tokens.muted)
        );
    }
}
