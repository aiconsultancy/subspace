//! Decision chips open the workspace's decision page. The passive hover
//! card reads the latest decision when shown, under the current account.

use std::time::{Duration, Instant};

use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyView, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Task,
    TestSupportExt as _, Window, div, rems,
};
use subspace_api_client::model::DecisionStatus;
use subspace_api_client::{Client, Error};
use subspace_client_irc::BufferKey;
use subspace_framework::contracts::decisions::{self, OpenDecision};
use subspace_framework::dashboard::contracts::{self, MessageKey};
use subspace_framework::dashboard::reference_card::{self, Treatment};
use subspace_framework::dashboard::{BoardNavigation, DashboardEvent, Tokens, Tone};
use subspace_framework::lucide::{IconName, SvgIcon};

use crate::{ID, Irc, back, hub::Hub};

const CAPABILITY_RETRY: Duration = Duration::from_secs(60);
const ACCOUNT_CHECK_INTERVAL: Duration = Duration::from_millis(250);
const CARD_WIDTH_REM: f32 = 320. / subspace_framework::dashboard::density::REFERENCE_REM;

/// One observation belongs to the account and origin that read it.
struct Observation {
    client: Option<Client>,
    supported: bool,
    checked: Instant,
}

#[derive(Default)]
pub(crate) struct Support {
    observed: Option<Observation>,
    read: Option<Task<()>>,
}

impl Support {
    pub(crate) fn client(&self) -> Option<&Client> {
        self.observed
            .as_ref()
            .filter(|observed| observed.supported)?
            .client
            .as_ref()
            .filter(|client| !client.account_changed())
    }

    pub(crate) fn probe(
        &mut self,
        connect: impl FnOnce() -> Result<Client, Error> + Send + 'static,
        cx: &mut Context<Hub>,
    ) {
        if self.read.is_some() || self.not_due() {
            return;
        }
        let read = cx.background_spawn(async move {
            let client = connect().ok();
            let supported = client
                .as_ref()
                .is_some_and(|client| client.supports_decision_identifiers() == Ok(true));
            Observation {
                client,
                supported,
                checked: Instant::now(),
            }
        });
        self.read = Some(cx.spawn(async move |hub, cx| {
            let observed = read.await;
            _ = hub.update(cx, |hub, cx| {
                hub.decision_support.read = None;
                hub.decision_support.observed = Some(observed);
                cx.notify();
            });
        }));
    }

    fn not_due(&self) -> bool {
        self.observed.as_ref().is_some_and(|observed| {
            let account_changed = observed
                .client
                .as_ref()
                .is_some_and(Client::account_changed);
            !account_changed
                && (observed.supported || observed.checked.elapsed() < CAPABILITY_RETRY)
        })
    }
}

pub(crate) fn available(hub: &Hub, cx: &App) -> bool {
    hub.decision_support.client().is_some()
        && contracts::navigation_target(ID, MessageKey::request::<OpenDecision>(), cx).is_some()
}

pub(crate) fn open(board: &Irc, identifier: &str, buffer: &BufferKey, cx: &mut Context<Irc>) {
    if board
        .hub
        .as_ref()
        .is_some_and(|hub| available(hub.read(cx), cx))
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

pub(crate) fn tooltip(identifier: &str, client: Client, cx: &mut App) -> AnyView {
    let identifier = identifier.to_owned();
    cx.new(|cx| Preview::new(identifier, client, cx)).into()
}

impl Preview {
    fn new(identifier: String, client: Client, cx: &mut Context<Self>) -> Self {
        let reference = identifier.clone();
        let read = cx.background_spawn(async move { read_preview(client, &reference) });
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

fn read_preview(client: Client, identifier: &str) -> Option<Loaded> {
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
