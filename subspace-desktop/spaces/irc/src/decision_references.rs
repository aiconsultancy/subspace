//! Decision chips open the workspace's decision page. The passive hover
//! card reads the latest decision when shown, under the current account.

use std::time::Duration;

use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyView, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Task,
    TestSupportExt as _, Window, div, rems,
};
use subspace_api_client::model::DecisionStatus;
use subspace_api_client::{Backend, Client};
use subspace_client_irc::BufferKey;
use subspace_framework::contracts::decisions::{self, OpenDecision};
use subspace_framework::dashboard::contracts::{self, MessageKey};
use subspace_framework::dashboard::reference_card::{self, Treatment};
use subspace_framework::dashboard::{BoardNavigation, DashboardEvent, Tokens, Tone};
use subspace_framework::lucide::{IconName, SvgIcon};

use crate::{ID, Irc, back};

const ACCOUNT_CHECK_INTERVAL: Duration = Duration::from_millis(250);
const CARD_WIDTH_REM: f32 = 320. / subspace_framework::dashboard::density::REFERENCE_REM;

pub(crate) fn available(cx: &App) -> bool {
    subspace_api_client::backend() == Backend::Subspace
        && contracts::navigation_target(ID, MessageKey::request::<OpenDecision>(), cx).is_some()
}

pub(crate) fn open(identifier: &str, buffer: &BufferKey, cx: &mut Context<Irc>) {
    if available(cx)
        && let Some(url) = decisions::link(identifier)
    {
        cx.emit(DashboardEvent::Navigate(BoardNavigation {
            responder: MessageKey::request::<OpenDecision>(),
            url: url.into(),
            origin: back::place(buffer),
        }));
    }
}

struct Loaded {
    account: Client,
    identifier: String,
    title: String,
    status: DecisionStatus,
}

enum State {
    Loading,
    Loaded(Box<Loaded>),
    Unavailable,
}

struct Preview {
    identifier: String,
    state: State,
    _read: Task<()>,
}

pub(crate) fn tooltip(identifier: &str, _: &mut Window, cx: &mut App) -> AnyView {
    let identifier = identifier.to_owned();
    cx.new(|cx| Preview::new(identifier, cx)).into()
}

impl Preview {
    fn new(identifier: String, cx: &mut Context<Self>) -> Self {
        let reference = identifier.clone();
        let read = cx.background_spawn(async move { read_preview(&reference) });
        let _read = cx.spawn(async move |preview, cx| {
            let Some(loaded) = read.await else {
                _ = preview.update(cx, |preview, cx| preview.show(State::Unavailable, cx));
                return;
            };
            let account = loaded.account.clone();
            if preview
                .update(cx, |preview, cx| {
                    preview.show(State::Loaded(Box::new(loaded)), cx)
                })
                .is_err()
            {
                return;
            }
            while !account.account_changed() {
                cx.background_executor().timer(ACCOUNT_CHECK_INTERVAL).await;
            }
            _ = preview.update(cx, |preview, cx| preview.show(State::Unavailable, cx));
        });
        Self {
            identifier,
            state: State::Loading,
            _read,
        }
    }

    fn show(&mut self, state: State, cx: &mut Context<Self>) {
        self.state = state;
        cx.notify();
    }
}

fn read_preview(identifier: &str) -> Option<Loaded> {
    let client = Client::new().ok()?;
    let detail = client.decision(identifier).ok()?;
    Some(Loaded {
        account: client,
        identifier: detail
            .item
            .identifier
            .unwrap_or_else(|| identifier.to_owned()),
        title: detail.item.title,
        status: detail.item.status,
    })
}

impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let content = match &self.state {
            State::Loaded(loaded) if !loaded.account.account_changed() => card(loaded, &tokens, cx),
            State::Loading => div().child(format!("Loading {}…", self.identifier)),
            _ => div().child("Decision unavailable"),
        };
        div()
            .id("irc-decision-hover-card")
            .role(gpui_kit::Role::Tooltip)
            .test_support()
            .font_family(cx.theme().font_family.clone())
            .m_3()
            .child(reference_card::frame(
                content,
                Treatment::Outline,
                &tokens,
                cx,
            ))
    }
}

fn card(detail: &Loaded, tokens: &Tokens, cx: &App) -> gpui_kit::Div {
    let (label, icon, tone) = status(detail.status);
    let identifier = detail.identifier.clone();
    v_flex()
        .w(rems(CARD_WIDTH_REM))
        .min_w_0()
        .gap_1()
        .child(
            h_flex()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .font_family(cx.theme().mono_font_family.clone())
                        .child(identifier.clone().test_text(identifier)),
                )
                .child(reference_card::pill(label, icon, tone, tokens)),
        )
        .child(
            div()
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .text_color(tokens.fg_strong)
                .whitespace_normal()
                .line_clamp(2)
                .child(detail.title.clone().test_text(detail.title.clone())),
        )
}

fn status(status: DecisionStatus) -> (&'static str, SvgIcon, Tone) {
    match status {
        DecisionStatus::Proposed => ("Proposed", IconName::CircleDot, Tone::Yellow),
        DecisionStatus::Accepted => ("Accepted", IconName::CircleCheck, Tone::Green),
        DecisionStatus::Rejected => ("Rejected", IconName::CircleX, Tone::Red),
        DecisionStatus::Superseded => ("Superseded", IconName::ArrowLeftRight, Tone::Muted),
        _ => ("Unknown status", IconName::CircleDashed, Tone::Muted),
    }
}
