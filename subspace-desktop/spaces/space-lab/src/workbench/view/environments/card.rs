//! One environment catalog card: header, tool/version meta, active-summary
//! and the provision action.

use gpui_kit::component::v_flex;
use gpui_kit::component::{Disableable as _, h_flex};
use gpui_kit::{
    Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;

use super::super::super::Workbench;
use super::super::dims::ENVIRONMENT_CARD_WIDTH;

pub(super) struct CardProps<'a> {
    pub(super) selected_id: &'a str,
    pub(super) busy: bool,
    pub(super) heading_font: &'a gpui_kit::SharedString,
    pub(super) t: &'a Tokens,
}

pub(super) fn environment_card(
    info: crate::contracts::EnvironmentInfo,
    props: CardProps<'_>,
    cx: &mut Context<Workbench>,
) -> impl IntoElement + use<> {
    let id = info.spec.id.clone();
    let selected = id == props.selected_id;
    let t = props.t;
    v_flex()
        .w(ENVIRONMENT_CARD_WIDTH)
        .p(t.pad_x)
        .gap_1()
        .border_1()
        .border_color(if selected { t.accent } else { t.hair })
        .rounded(t.surface_radius)
        .bg(t.pane)
        .id(SharedString::from(format!("workbench-env-{id}")))
        .on_click(card_select_handler(&info, cx))
        .child(card_header(&info, props.heading_font, t))
        .child(card_meta(&info, t))
        .children(info.summary.starts_with("Active ").then(|| {
            div()
                .text_size(t.caption_size())
                .text_color(t.orange)
                .child(info.summary.clone())
        }))
        .child(card_provision_action(&id, props.busy, cx))
}

fn card_select_handler(
    info: &crate::contracts::EnvironmentInfo,
    cx: &mut Context<Workbench>,
) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App) + 'static {
    let id = info.spec.id.clone();
    let version = info.spec.version.clone();
    cx.listener(move |this, _, window, cx| {
        this.selected_env = Some(id.clone());
        this.inputs
            .version_input
            .update(cx, |input, cx| input.set_value(version.clone(), window, cx));
        cx.notify();
    })
}

fn card_header(
    info: &crate::contracts::EnvironmentInfo,
    heading_font: &gpui_kit::SharedString,
    t: &Tokens,
) -> impl IntoElement {
    h_flex()
        .justify_between()
        .child(
            div()
                .font_family(heading_font.clone())
                .font_weight(FontWeight::MEDIUM)
                .text_size(t.heading_font)
                .text_color(t.fg_strong)
                .child(info.spec.title.clone()),
        )
        .child(
            div()
                .text_size(t.caption_size())
                .text_color(if info.ready { t.green } else { t.muted })
                .child(if info.ready {
                    "ready"
                } else {
                    "not provisioned"
                }),
        )
}

fn card_meta(info: &crate::contracts::EnvironmentInfo, t: &Tokens) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(
            div()
                .text_size(t.secondary_size())
                .text_color(t.cyan)
                .child(info.spec.tools.join(" · ")),
        )
        .child(
            div()
                .text_size(t.secondary_size())
                .text_color(t.muted)
                .child(format!(
                    "{:?} {} · {} packages · {}",
                    info.spec.runtime,
                    info.spec.version,
                    info.spec.packages.len(),
                    info.spec.package_manager
                )),
        )
}

fn card_provision_action(id: &str, busy: bool, cx: &mut Context<Workbench>) -> impl IntoElement {
    h_flex().flex_wrap().child(
        action_button(
            SharedString::from(format!("workbench-provision-{id}")),
            Intent::Primary,
            Some(IconName::Download),
            "Provision",
        )
        .disabled(busy)
        .on_click(cx.listener({
            let id = id.to_owned();
            move |this, _, _, cx| this.provision(id.clone(), cx)
        })),
    )
}
