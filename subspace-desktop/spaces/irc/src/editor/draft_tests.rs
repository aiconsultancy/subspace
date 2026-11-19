use serde_json::{Map, Value, json};

use super::super::ids::{new_id, slug, unique_id};
use super::super::validate::{Field, is_channel, is_nick, problems};
use super::{
    NetworkText, SaslMode, SecretEdit, Secrets, from_saved, readable, saved_passwords, split_list,
    to_json,
};

fn object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn keep() -> Secrets {
    Secrets {
        server: SecretEdit::Keep,
        sasl: SecretEdit::Keep,
    }
}

fn saved() -> Map<String, Value> {
    object(json!({
        "id": "example", "name": "Example Network", "host": "irc.example.net",
        "port": 6697, "tls": true, "nickname": "fixture", "altNicks": ["fixture_"],
        "password": "server-secret",
        "sasl": {"username": "fixture", "password": "account-secret", "future": 1},
        "channels": ["#one", "&two"], "highlights": ["rocket"], "futureKey": {"keep": true}
    }))
}

#[test]
fn a_saved_network_round_trips_with_its_secrets_and_unknown_keys() {
    let raw = saved();
    let text = from_saved(&raw);
    assert_eq!(text.sasl, SaslMode::Plain);
    assert_eq!(text.channels, "#one, &two");
    assert_eq!(saved_passwords(&raw), (true, true));
    let written = to_json(&raw, "example", &text, &keep());
    assert_eq!(written["password"], "server-secret");
    assert_eq!(written["sasl"]["password"], "account-secret");
    assert_eq!(written["sasl"]["future"], 1);
    assert_eq!(written["futureKey"], json!({"keep": true}));
    assert_eq!(written["highlights"], json!(["rocket"]));
    assert_eq!(written["channels"], json!(["#one", "&two"]));
    assert!(readable(&written).is_ok());
}

#[test]
fn secret_edits_keep_replace_or_remove() {
    assert_eq!(SecretEdit::new(true, false, ""), SecretEdit::Keep);
    assert_eq!(SecretEdit::new(true, true, ""), SecretEdit::Remove);
    assert_eq!(SecretEdit::new(false, false, ""), SecretEdit::Remove);
    assert_eq!(
        SecretEdit::new(true, true, "new"),
        SecretEdit::Replace("new".into())
    );
    let raw = saved();
    let text = from_saved(&raw);
    let secrets = Secrets {
        server: SecretEdit::Remove,
        sasl: SecretEdit::Replace("changed".into()),
    };
    let written = to_json(&raw, "example", &text, &secrets);
    assert!(written.get("password").is_none());
    assert_eq!(written["sasl"]["password"], "changed");
}

#[test]
fn sasl_modes_write_their_own_keys() {
    let raw = saved();
    let mut text = from_saved(&raw);
    text.password_from_file = true;
    text.password_file = "/home/fixture/irc-password".into();
    let written = to_json(&raw, "example", &text, &keep());
    assert_eq!(
        written["sasl"]["passwordFile"],
        "/home/fixture/irc-password"
    );
    assert!(written["sasl"].get("password").is_none());
    text.sasl = SaslMode::External;
    text.client_cert = "/home/fixture/client.pem".into();
    let written = to_json(&raw, "example", &text, &keep());
    assert_eq!(written["sasl"]["external"], true);
    assert!(written["sasl"].get("passwordFile").is_none());
    text.sasl = SaslMode::None;
    assert!(
        to_json(&raw, "example", &text, &keep())
            .get("sasl")
            .is_none()
    );
}

#[test]
fn required_values_and_formats_are_checked() {
    let empty = NetworkText::default();
    let found = problems(&empty, &keep());
    let fields: Vec<Field> = found.iter().map(|problem| problem.field).collect();
    assert_eq!(fields, [Field::Host, Field::Nickname]);
    assert!(found.iter().all(|problem| problem.required));
    let text = NetworkText {
        host: "irc.example.net".into(),
        port: "70000".into(),
        nickname: "9lives".into(),
        channels: "#ok, general".into(),
        ca_cert: "relative/ca.pem".into(),
        ..NetworkText::default()
    };
    let fields: Vec<Field> = problems(&text, &keep())
        .iter()
        .map(|problem| problem.field)
        .collect();
    assert_eq!(
        fields,
        [Field::Port, Field::CaCert, Field::Nickname, Field::Channels]
    );
}

#[test]
fn sign_in_needs_a_password_or_a_password_file() {
    let text = NetworkText {
        host: "irc.example.net".into(),
        nickname: "fixture".into(),
        sasl: SaslMode::Plain,
        sasl_account: "fixture".into(),
        ..NetworkText::default()
    };
    let none = Secrets {
        server: SecretEdit::Remove,
        sasl: SecretEdit::Remove,
    };
    assert_eq!(problems(&text, &none)[0].field, Field::SaslPassword);
    let from_file = NetworkText {
        password_from_file: true,
        password_file: "/home/fixture/pw".into(),
        ..text.clone()
    };
    assert!(problems(&from_file, &none).is_empty());
    let external = NetworkText {
        sasl: SaslMode::External,
        tls: false,
        ..text
    };
    assert_eq!(problems(&external, &none)[0].field, Field::ClientCert);
}

#[test]
fn nicks_channels_and_lists_follow_irc_rules() {
    assert!(is_nick("fixture") && is_nick("[away]") && is_nick("a-b_c"));
    assert!(!is_nick("1st") && !is_nick("two words") && !is_nick(""));
    assert!(is_channel("#subspace") && is_channel("&local"));
    assert!(!is_channel("subspace") && !is_channel("#") && !is_channel("#a,b"));
    assert_eq!(split_list("#a, #b\n#c  #d,,"), ["#a", "#b", "#c", "#d"]);
}

#[test]
fn new_ids_come_from_the_name_and_stay_unique() {
    assert_eq!(slug("Example.Net IRC"), "example-net-irc");
    assert_eq!(unique_id("example", &["example".into()]), "example-2");
    assert_eq!(
        unique_id("example", &["example".into(), "example-2".into()]),
        "example-3"
    );
    assert_eq!(unique_id("", &[]), "network");
    let host_only = NetworkText {
        host: "irc.example.net".into(),
        ..NetworkText::default()
    };
    assert_eq!(new_id(&host_only, &[]), "irc-example-net");
}
