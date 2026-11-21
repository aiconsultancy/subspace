//! The Environments tab: the environment catalog, the active-draft summary,
//! and the version-pin/package/tool action row.

mod card;
mod pin_row;

use gpui_kit::component::Disableable as _;
use gpui_kit::component::h_flex;
use gpui_kit::component::v_flex;
use gpui_kit::{Context, FontWeight, IntoElement, ParentElement as _, Styled as _, Window, div};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::typography;

use super::super::Workbench;
use card::{CardProps, environment_card};
use pin_row::pin_and_package_row;

pub(super) fn environment_panel(
    this: &mut Workbench,
    window: &mut Window,
    cx: &mut Context<Workbench>,
    t: &Tokens,
) -> impl IntoElement {
    let heading_font = typography::heading_font(cx);
    let selected_id = this
        .selected_env
        .clone()
        .unwrap_or_else(|| this.draft.environment.clone());
    let busy = this.jobs.busy;
    let mut rows = Vec::new();
    for info in this.catalog.clone() {
        let props = CardProps {
            selected_id: &selected_id,
            busy,
            heading_font: &heading_font,
            t,
        };
        rows.push(environment_card(info, props, cx));
    }

    v_flex()
        .p(t.pad_x)
        .gap_2()
        .child(
            div()
                .font_family(heading_font.clone())
                .font_weight(FontWeight::MEDIUM)
                .text_size(t.heading_font)
                .text_color(t.fg_strong)
                .child("Environments"),
        )
        .child(h_flex().gap_3().flex_wrap().children(rows))
        .child(
            div()
                .text_size(t.secondary_size())
                .text_color(t.orange)
                .child(
                    "Tool and package installation is a trusted, network-enabled host \
                     provisioning step in this spike. Worker execution uses MXC. Python's \
                     historical releases currently lack the mise attestation receipt used by \
                     newer releases.",
                ),
        )
        .child(pin_and_package_row(this, window, cx, t))
}

pub(super) fn active_environment(
    this: &Workbench,
    cx: &mut Context<Workbench>,
    t: &Tokens,
) -> impl IntoElement {
    let id = this.draft.environment.clone();
    let info = this.catalog.iter().find(|info| info.spec.id == id);
    let description = info
        .map(|info| {
            format!(
                "{:?} {} · {} · {}",
                info.spec.runtime,
                info.spec.version,
                info.spec.package_manager,
                if info.ready {
                    "ready"
                } else {
                    "needs provisioning"
                }
            )
        })
        .unwrap_or_else(|| "Static canvas snapshot · no executable recipe".into());
    v_flex()
        .gap_2()
        .pb_3()
        .border_b_1()
        .border_color(t.line)
        .child(
            h_flex()
                .gap_2()
                .flex_wrap()
                .child(div().text_color(t.cyan).child(description)),
        )
        .children((!id.is_empty()).then(|| {
            h_flex().child(
                action_button(
                    "workbench-provision-active",
                    Intent::Neutral,
                    Some(IconName::Download),
                    "Provision profile",
                )
                .disabled(this.jobs.busy)
                .on_click(cx.listener(move |this, _, _, cx| this.provision(id.clone(), cx))),
            )
        }))
        .child(
            div()
                .text_size(t.secondary_size())
                .text_color(t.muted)
                .child(if this.networked {
                    "MXC proxy-only network · private packages · no credentials"
                } else {
                    "Offline MXC execution · private packages · no credentials"
                }),
        )
}
