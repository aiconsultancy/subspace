//! One network's form on the IRC Settings page.

use gpui_kit::component::input::InputState;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, ElementId, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::fields::{self, InputRole};
use subspace_framework::dashboard::secret::{PASSWORD, SecretActions, SecretField, secret_field};
use subspace_framework::dashboard::toggle::{switch, toggle_row};
use subspace_framework::dashboard::{Tokens, anchored};
use subspace_framework::lucide::IconName;

use super::IrcSettings;
use super::form::{NetworkForm, Switches};
use super::validate::{Field, Problem};

const INVALID_CERTS_NOTE: &str = "Only for a local test server. Any certificate is accepted, so the connection can be intercepted.";

/// What one form row needs: its network, problems and lock.
struct Form<'a> {
    network: &'a NetworkForm,
    problems: &'a [Problem],
    locked: bool,
    tokens: &'a Tokens,
}

impl Form<'_> {
    /// The problem shown under `field`. Missing values wait for Done.
    fn problem(&self, field: Field) -> Option<&Problem> {
        self.problems
            .iter()
            .find(|problem| problem.field == field && problem.is_shown(self.network.checked))
    }

    fn with_problem(&self, row: Div, field: Option<Field>) -> Div {
        let problem = field.and_then(|field| self.problem(field));
        row.children(problem.map(|problem| {
            fields::field_error(
                self.tokens,
                ElementId::Name(format!("irc-problem-{:?}", problem.field).into()),
                &problem.message,
            )
        }))
    }

    fn text(&self, label: &'static str, input: &Entity<InputState>, field: Option<Field>) -> Div {
        let control = fields::input(input, InputRole::Configuration, label)
            .id(label)
            .disabled(self.locked);
        self.with_problem(fields::form_field(label, control), field)
    }

    fn path(&self, label: &'static str, input: &Entity<InputState>, field: Field) -> Div {
        let control = fields::path_input(
            input,
            label,
            format!("Choose the {}", label.to_lowercase()),
            self.locked,
        );
        self.with_problem(fields::form_field(label, control), Some(field))
    }
}

/// The form of `network`, one of `editor`'s networks.
pub(super) fn render(
    editor: &IrcSettings,
    network: &NetworkForm,
    tokens: &Tokens,
    cx: &mut Context<IrcSettings>,
) -> AnyElement {
    let key = network.key;
    let problems = editor.problems(key, cx);
    let form = Form {
        network,
        problems: &problems,
        locked: editor.is_locked(),
        tokens,
    };
    v_flex()
        .id(("irc-network-form", key))
        .test_support()
        .gap_4()
        .child(fields::heading(network.title(cx), cx))
        .child(server_group(&form, cx))
        .child(identity_group(&form))
        .child(sign_in_group(editor, &form, cx))
        .child(channels_group(&form, cx))
        .child(
            h_flex().child(
                action_button(
                    "irc-network-done",
                    Intent::Neutral,
                    Some(IconName::Check),
                    "Done",
                )
                .on_click(cx.listener(move |editor, _, _, cx| editor.done(key, cx))),
            ),
        )
        .into_any_element()
}

fn group(title: &'static str) -> Div {
    v_flex()
        .gap_3()
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
}

fn server_group(form: &Form<'_>, cx: &mut Context<IrcSettings>) -> Div {
    let inputs = &form.network.inputs;
    let switches = form.network.switches;
    group("Server")
        .child(form.text("Name", &inputs.name, None))
        .child(form.text("Host", &inputs.host, Some(Field::Host)))
        .child(form.text("Port", &inputs.port, Some(Field::Port)))
        .child(switch_row(
            form,
            ("irc-tls", "Use TLS"),
            switches.tls,
            |switches, on| switches.tls = on,
            cx,
        ))
        .when(switches.tls, |group| {
            group
                .child(
                    v_flex()
                        .gap_1()
                        .child(switch_row(
                            form,
                            ("irc-accept-invalid-certs", "Accept invalid certificates"),
                            switches.accept_invalid_certs,
                            |switches, on| switches.accept_invalid_certs = on,
                            cx,
                        ))
                        .child(fields::note(form.tokens, INVALID_CERTS_NOTE)),
                )
                .child(form.path("CA certificate", &inputs.ca_cert, Field::CaCert))
                .child(form.path("Client certificate", &inputs.client_cert, Field::ClientCert))
                .child(form.path("Client key", &inputs.client_key, Field::ClientKey))
        })
}

fn identity_group(form: &Form<'_>) -> Div {
    let inputs = &form.network.inputs;
    group("Identity")
        .child(form.text("Nickname", &inputs.nickname, Some(Field::Nickname)))
        .child(form.text(
            "Alternative nicknames",
            &inputs.alt_nicks,
            Some(Field::AltNicks),
        ))
        .child(form.text("Username", &inputs.username, Some(Field::Username)))
        .child(form.text("Real name", &inputs.realname, None))
}

fn sign_in_group(editor: &IrcSettings, form: &Form<'_>, cx: &mut Context<IrcSettings>) -> Div {
    let network = form.network;
    let mode = network.sasl_mode(cx);
    group("Sign-in")
        .child(password(editor, form, ("irc-server-password", "Server password"), false, cx))
        .child(fields::note(form.tokens, "Only for servers that ask for one. Most networks sign in with an account instead."))
        .child(fields::form_field(
            "Account sign-in (SASL)",
            anchored::select(&network.sasl)
                .id("irc-sasl")
                .accessibility_label("Account sign-in (SASL)")
                .disabled(form.locked),
        ))
        .when(mode.is_plain(), |group| plain_sign_in(editor, form, group, cx))
        .when(mode.is_external(), |group| {
            group
                .child(form.text("Account", &network.inputs.sasl_account, None))
                .child(fields::note(form.tokens, "Signs in with the client certificate under Server. Leave the account empty to use the certificate's own."))
        })
}

fn plain_sign_in(
    editor: &IrcSettings,
    form: &Form<'_>,
    group: Div,
    cx: &mut Context<IrcSettings>,
) -> Div {
    let network = form.network;
    let from_file = network.switches.password_from_file;
    group
        .child(form.text(
            "Account",
            &network.inputs.sasl_account,
            Some(Field::SaslAccount),
        ))
        .child(switch_row(
            form,
            ("irc-password-from-file", "Read the password from a file"),
            from_file,
            |switches, on| switches.password_from_file = on,
            cx,
        ))
        .when(from_file, |group| {
            group.child(
                form.path(
                    "Password file",
                    &network.inputs.password_file,
                    Field::PasswordFile,
                )
                .child(fields::note(
                    form.tokens,
                    "Subspace reads the file each time it connects and never stores the password.",
                )),
            )
        })
        .when(network.switches.stores_password(), |group| {
            let row = password(
                editor,
                form,
                ("irc-sasl-password", "Account password"),
                true,
                cx,
            );
            group.child(form.with_problem(v_flex().child(row), Some(Field::SaslPassword)))
        })
}

fn channels_group(form: &Form<'_>, cx: &mut Context<IrcSettings>) -> Div {
    let inputs = &form.network.inputs;
    group("Channels")
        .child(
            form.text("Channels to join", &inputs.channels, Some(Field::Channels))
                .child(fields::note(form.tokens, "Joined after every connect. Separate channels with commas, such as #subspace, #rust.")),
        )
        .child(switch_row(
            form,
            ("irc-auto-connect", "Connect on startup"),
            form.network.switches.auto_connect,
            |switches, on| switches.auto_connect = on,
            cx,
        ))
}

fn switch_row(
    form: &Form<'_>,
    (id, label): (&'static str, &'static str),
    on: bool,
    set: fn(&mut Switches, bool),
    cx: &mut Context<IrcSettings>,
) -> Div {
    let key = form.network.key;
    toggle_row(
        label,
        switch((id, key), cx)
            .checked(on)
            .disabled(form.locked)
            .on_click(cx.listener(move |editor, on: &bool, _, cx| {
                let on = *on;
                editor.set_switch(key, |switches| set(switches, on), cx);
            })),
    )
}

fn password(
    editor: &IrcSettings,
    form: &Form<'_>,
    (id, label): (&'static str, &'static str),
    sasl: bool,
    cx: &mut Context<IrcSettings>,
) -> AnyElement {
    let key = form.network.key;
    let secret = form.network.secret(sasl);
    let row_id = ElementId::NamedInteger(SharedString::from(id), key);
    let hint = row_id.clone();
    secret_field(
        SecretField {
            hinted: editor.is_hinted(&row_id),
            id: row_id,
            label: label.into(),
            input: &secret.input,
            clear: secret.clear,
            saved: secret.saved,
            locked: form.locked,
            words: &PASSWORD,
        },
        SecretActions {
            on_toggle: cx
                .listener(move |editor, _, window, cx| editor.toggle_clear(key, sasl, window, cx)),
            on_hint: cx.listener(move |editor, _, _, cx| editor.show_hint(hint.clone(), cx)),
        },
        form.tokens,
    )
}
