//! **Settings › Fleets**: each fleet the person's initiatives belong to,
//! with the address the Subspace API records and its state, a field to
//! paste and save its token, **Test connection** and **Clear token**.
//! Saving and clearing apply at once, outside Settings' Save and Cancel.
//! The field is masked and emptied as soon as a save starts, so a token
//! never stays in the page.

use std::collections::BTreeMap;

use gpui_kit::component::input::InputState;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AnyView, App, AppContext as _, Context, Div, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Styled as _, Subscription, Window, div,
};
use subspace_framework::core::Secret;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::fields::{self, InputRole};
use subspace_framework::lucide::IconName;

use crate::model::FleetOverview;
use crate::{FleetApi, FleetFollow, FleetInfo, FleetState, TokenError};

/// The page's introduction.
pub const INTRODUCTION: &str = "Fleets come from your initiatives in the Subspace API, which records each fleet's Fleet API address. Paste each fleet's token once: Subspace keeps it on this computer, in the system keychain, and sends it only to the address it was saved for. Every Space that shows a fleet, including Fleet Console, uses it. Saving or clearing a token applies at once; Settings' Save and Cancel do not undo it.";
const NO_FLEETS: &str = "None of your initiatives is assigned to a fleet yet. Sign in under Settings › Account to list your initiatives.";

/// Creates the retained page for the product's Settings extension.
// qual:api - the product constructs this view through SettingsExtension.
pub fn create(window: &mut Window, cx: &mut App) -> AnyView {
    let service = FleetApi::shared(cx);
    let catalog = subspace_api_client::initiatives::Catalog::shared(cx);
    catalog.update(cx, |catalog, cx| {
        if !catalog.has_memberships() {
            catalog.refresh(cx);
        }
    });
    cx.new(|cx| FleetSettings::new(service, window, cx)).into()
}

/// What the page says after an action on one fleet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Working(&'static str),
    Done(String),
    Failed(String),
}

struct Row {
    input: Entity<InputState>,
    outcome: Option<Outcome>,
}

pub(crate) struct FleetSettings {
    service: Entity<FleetApi>,
    rows: BTreeMap<String, Row>,
    /// Every fleet is followed while the page is open, so its state is
    /// the live one.
    follows: Vec<FleetFollow>,
    _service: Subscription,
}

impl FleetSettings {
    pub(crate) fn new(
        service: Entity<FleetApi>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe = cx.observe_in(&service, window, |this: &mut Self, _, window, cx| {
            this.sync(window, cx);
        });
        let mut page = Self {
            service,
            rows: BTreeMap::new(),
            follows: Vec::new(),
            _service: observe,
        };
        page.sync(window, cx);
        page
    }

    /// One row per fleet, each followed.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fleets = self.service.read(cx).fleets();
        self.rows
            .retain(|id, _| fleets.iter().any(|fleet| &fleet.record.id == id));
        for fleet in &fleets {
            if !self.rows.contains_key(&fleet.record.id) {
                let input = cx.new(|cx| InputState::new(window, cx).masked(true));
                self.rows.insert(
                    fleet.record.id.clone(),
                    Row {
                        input,
                        outcome: None,
                    },
                );
            }
        }
        let ids: Vec<&str> = fleets
            .iter()
            .map(|fleet| fleet.record.id.as_str())
            .collect();
        let followed: Vec<&str> = self.follows.iter().map(FleetFollow::fleet).collect();
        if followed != ids {
            self.follows = self.service.update(cx, |service, cx| {
                ids.iter().map(|fleet| service.follow(fleet, cx)).collect()
            });
        }
        cx.notify();
    }

    fn typed(&self, fleet: &str, cx: &App) -> Option<Secret> {
        let text = self.rows.get(fleet)?.input.read(cx).value();
        let text = text.trim();
        (!text.is_empty()).then(|| Secret::new(text))
    }

    fn set_outcome(&mut self, fleet: &str, outcome: Outcome, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.get_mut(fleet) {
            row.outcome = Some(outcome);
            cx.notify();
        }
    }

    pub(crate) fn save(&mut self, fleet: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(token) = self.typed(fleet, cx) else {
            self.set_outcome(
                fleet,
                Outcome::Failed("Paste the token to save it.".into()),
                cx,
            );
            return;
        };
        if let Some(row) = self.rows.get(fleet) {
            row.input
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        let address = self
            .service
            .read(cx)
            .fleet(fleet)
            .and_then(|info| info.address);
        let task = self
            .service
            .update(cx, |service, cx| service.save_token(fleet, token, cx));
        self.set_outcome(fleet, Outcome::Working("Saving the token…"), cx);
        let id = fleet.to_owned();
        cx.spawn(async move |this, cx| {
            let outcome = match (task.await, address) {
                (Ok(()), Some(address)) => Outcome::Done(format!(
                    "Saved. Subspace sends this token only to {}.",
                    address.as_str()
                )),
                (Ok(()), None) => Outcome::Done("Saved.".into()),
                (Err(error), _) => Outcome::Failed(error.to_string()),
            };
            _ = this.update(cx, |this, cx| this.set_outcome(&id, outcome, cx));
        })
        .detach();
    }

    pub(crate) fn test(&mut self, fleet: &str, cx: &mut Context<Self>) {
        let token = self.typed(fleet, cx);
        let task = self
            .service
            .update(cx, |service, cx| service.test_connection(fleet, token, cx));
        self.set_outcome(fleet, Outcome::Working("Testing the connection…"), cx);
        let id = fleet.to_owned();
        cx.spawn(async move |this, cx| {
            let outcome = tested(task.await);
            _ = this.update(cx, |this, cx| this.set_outcome(&id, outcome, cx));
        })
        .detach();
    }

    pub(crate) fn clear(&mut self, fleet: &str, cx: &mut Context<Self>) {
        let task = self
            .service
            .update(cx, |service, cx| service.clear_token(fleet, cx));
        let id = fleet.to_owned();
        cx.spawn(async move |this, cx| {
            let outcome = match task.await {
                Ok(()) => Outcome::Done("Token cleared from this computer.".into()),
                Err(error) => Outcome::Failed(error.to_string()),
            };
            _ = this.update(cx, |this, cx| this.set_outcome(&id, outcome, cx));
        })
        .detach();
    }

    #[cfg(test)]
    pub(crate) fn outcome(&self, fleet: &str) -> Option<&Outcome> {
        self.rows.get(fleet)?.outcome.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn input(&self, fleet: &str) -> Option<&Entity<InputState>> {
        self.rows.get(fleet).map(|row| &row.input)
    }
}

/// The Fleet Console's words for a test connection.
fn tested(result: Result<FleetOverview, TokenError>) -> Outcome {
    match result {
        Ok(fleet) => {
            let ships = fleet.ships.len();
            let version = fleet
                .gateway
                .as_ref()
                .map_or("unknown", |gateway| gateway.version.as_str());
            Outcome::Done(format!(
                "Connected to “{}”: {ships} ship{}, Stargate {version}.",
                fleet.name,
                if ships == 1 { "" } else { "s" }
            ))
        }
        Err(error) => Outcome::Failed(error.to_string()),
    }
}

/// Whether a token is saved for the fleet, whatever its connection.
fn has_token(state: &FleetState) -> bool {
    !matches!(
        state,
        FleetState::NotConfigured
            | FleetState::InvalidAddress
            | FleetState::Checking
            | FleetState::NoToken
            | FleetState::StoreUnavailable
    )
}

impl Render for FleetSettings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let fleets = self.service.read(cx).fleets();
        v_flex()
            .id("fleet-settings")
            .min_w_0()
            .gap_4()
            .child(fields::note(&tokens, INTRODUCTION))
            .children(fleets.is_empty().then(|| fields::note(&tokens, NO_FLEETS)))
            .children(fleets.iter().map(|fleet| self.fleet(fleet, &tokens, cx)))
    }
}

impl FleetSettings {
    fn fleet(&self, fleet: &FleetInfo, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.rows.get(&fleet.record.id) else {
            return div().into_any_element();
        };
        let address = fleet
            .record
            .address
            .clone()
            .unwrap_or_else(|| "No Fleet API address recorded".into());
        v_flex()
            .gap_2()
            .pt_3()
            .border_t_1()
            .border_color(tokens.line)
            .child(fields::heading(fleet.record.name.clone(), cx))
            .child(fields::form_field(
                "Fleet API address",
                fields::note(tokens, address),
            ))
            .child(status(&fleet.state, tokens))
            .children(
                fleet
                    .address
                    .is_some()
                    .then(|| Self::token_controls(fleet, row, cx)),
            )
            .children(
                row.outcome
                    .as_ref()
                    .map(|outcome| outcome_line(&fleet.record.id, outcome, tokens)),
            )
            .into_any_element()
    }

    fn token_controls(fleet: &FleetInfo, row: &Row, cx: &mut Context<Self>) -> Div {
        let saved = has_token(&fleet.state);
        let label = if saved {
            "Replace token"
        } else {
            "Fleet API token"
        };
        let working = matches!(row.outcome, Some(Outcome::Working(_)));
        let id = fleet.record.id.clone();
        let (save, test, clear) = (id.clone(), id.clone(), id.clone());
        v_flex()
            .gap_2()
            .child(fields::form_field(
                label,
                fields::input(&row.input, InputRole::Configuration, label)
                    .id(format!("fleet-token-{id}"))
                    .disabled(working),
            ))
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        action_button(
                            format!("fleet-save-{id}"),
                            Intent::Neutral,
                            Some(IconName::KeyRound),
                            "Save token",
                        )
                        .disabled(working)
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.save(&save, window, cx)),
                        ),
                    )
                    .child(
                        action_button(
                            format!("fleet-test-{id}"),
                            Intent::Neutral,
                            Some(IconName::PlugZap),
                            "Test connection",
                        )
                        .disabled(working)
                        .on_click(cx.listener(move |this, _, _, cx| this.test(&test, cx))),
                    )
                    .children(saved.then(|| {
                        action_button(
                            format!("fleet-clear-{id}"),
                            Intent::Quiet,
                            Some(IconName::Eraser),
                            "Clear token",
                        )
                        .disabled(working)
                        .on_click(cx.listener(move |this, _, _, cx| this.clear(&clear, cx)))
                    })),
            )
    }
}

fn status(state: &FleetState, tokens: &Tokens) -> Div {
    let colour = if state.is_live() || *state == FleetState::Ready {
        tokens.green
    } else if state.help().is_some() && *state != FleetState::NoToken {
        tokens.red
    } else {
        tokens.muted
    };
    v_flex()
        .gap_1()
        .child(
            div()
                .text_sm()
                .text_color(colour)
                .child(format!("Status: {}", state.label())),
        )
        .children(state.help().map(|help| fields::note(tokens, help)))
        .children(match state {
            FleetState::AddressChanged {
                saved_for: Some(saved_for),
            } => Some(fields::note(
                tokens,
                format!("The saved token was for {saved_for}."),
            )),
            _ => None,
        })
}

fn outcome_line(fleet: &str, outcome: &Outcome, tokens: &Tokens) -> Div {
    match outcome {
        Outcome::Working(text) => fields::note(tokens, *text),
        Outcome::Done(text) => fields::note(tokens, text.clone()),
        Outcome::Failed(text) => fields::field_error(tokens, format!("fleet-error-{fleet}"), text),
    }
}

#[cfg(test)]
mod tests;
