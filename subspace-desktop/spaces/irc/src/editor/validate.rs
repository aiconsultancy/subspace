//! What a network draft must fix before Save: missing values and values
//! IRC servers refuse. Pure data: no GPUI.

use super::draft::{NetworkText, SaslMode, Secrets, split_list};

/// The form field a problem belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    Host,
    Port,
    Nickname,
    AltNicks,
    Username,
    Channels,
    CaCert,
    ClientCert,
    ClientKey,
    SaslAccount,
    SaslPassword,
    PasswordFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Problem {
    pub field: Field,
    pub message: String,
    /// A missing value, which the form shows only after Done or Save.
    pub required: bool,
}

impl Problem {
    /// Missing values show once the form was `checked`; others always.
    pub fn is_shown(&self, checked: bool) -> bool {
        checked || !self.required
    }
}

fn problem(field: Field, message: impl Into<String>) -> Problem {
    Problem {
        field,
        message: message.into(),
        required: false,
    }
}

fn missing(field: Field, message: &str) -> Problem {
    Problem {
        required: true,
        ..problem(field, message)
    }
}

/// Every problem in `text`, in form order.
pub(crate) fn problems(text: &NetworkText, secrets: &Secrets) -> Vec<Problem> {
    let mut found = Vec::new();
    found.extend(server_problems(text));
    found.extend(identity_problems(text));
    found.extend(sign_in_problems(text, secrets));
    found.extend(channel_problems(&text.channels));
    found
}

fn server_problems(text: &NetworkText) -> Vec<Problem> {
    let mut found = Vec::new();
    if text.host.trim().is_empty() {
        found.push(missing(Field::Host, "Enter the server's host name."));
    } else if text.host.trim().contains(char::is_whitespace) {
        found.push(problem(Field::Host, "A host name has no spaces."));
    }
    if parse_port(&text.port).is_none() {
        found.push(problem(Field::Port, "Use a port from 1 to 65535."));
    }
    for (field, path) in [
        (Field::CaCert, &text.ca_cert),
        (Field::ClientCert, &text.client_cert),
        (Field::ClientKey, &text.client_key),
    ] {
        if !is_absolute_or_empty(path) {
            found.push(problem(
                field,
                "Use a full path, such as /Users/you/irc/cert.pem.",
            ));
        }
    }
    found
}

fn identity_problems(text: &NetworkText) -> Vec<Problem> {
    let mut found = Vec::new();
    let nickname = text.nickname.trim();
    if nickname.is_empty() {
        found.push(missing(Field::Nickname, "Enter a nickname."));
    } else if !is_nick(nickname) {
        found.push(problem(Field::Nickname, NICK_RULE));
    }
    if split_list(&text.alt_nicks)
        .iter()
        .any(|nick| !is_nick(nick))
    {
        found.push(problem(Field::AltNicks, NICK_RULE));
    }
    if text
        .username
        .trim()
        .contains(|character: char| character.is_whitespace() || character == '@')
    {
        found.push(problem(Field::Username, "A user name has no spaces or @."));
    }
    found
}

const NICK_RULE: &str =
    "A nickname starts with a letter or one of [ ] \\ ` _ ^ { | } and has no spaces.";

fn sign_in_problems(text: &NetworkText, secrets: &Secrets) -> Vec<Problem> {
    match text.sasl {
        SaslMode::None => Vec::new(),
        SaslMode::Plain => plain_problems(text, secrets),
        SaslMode::External => external_problems(text),
    }
}

fn plain_problems(text: &NetworkText, secrets: &Secrets) -> Vec<Problem> {
    let mut found = Vec::new();
    if text.sasl_account.trim().is_empty() {
        found.push(missing(Field::SaslAccount, "Enter the account name."));
    }
    let file = text.password_file.trim();
    let password = match (text.password_from_file, file.is_empty()) {
        (true, true) => Some(missing(
            Field::PasswordFile,
            "Choose the file that holds the password.",
        )),
        (true, false) if !std::path::Path::new(file).is_absolute() => Some(problem(
            Field::PasswordFile,
            "Use a full path to the password file.",
        )),
        (false, _) if !secrets.sasl.is_set() => Some(missing(
            Field::SaslPassword,
            "Enter the account password, or read it from a file.",
        )),
        _ => None,
    };
    found.extend(password);
    found
}

fn external_problems(text: &NetworkText) -> Vec<Problem> {
    if !text.tls {
        vec![problem(Field::ClientCert, "SASL EXTERNAL needs TLS.")]
    } else if text.client_cert.trim().is_empty() {
        vec![missing(
            Field::ClientCert,
            "SASL EXTERNAL needs a client certificate.",
        )]
    } else {
        Vec::new()
    }
}

fn channel_problems(channels: &str) -> Vec<Problem> {
    split_list(channels)
        .iter()
        .find(|channel| !is_channel(channel))
        .map(|channel| {
            problem(
                Field::Channels,
                format!("“{channel}” is not a channel name. Channel names start with # or &."),
            )
        })
        .into_iter()
        .collect()
}

pub(crate) fn parse_port(text: &str) -> Option<u16> {
    text.trim().parse::<u16>().ok().filter(|port| *port > 0)
}

fn is_absolute_or_empty(path: &str) -> bool {
    let path = path.trim();
    path.is_empty() || std::path::Path::new(path).is_absolute()
}

/// RFC 2812 nick characters: a letter or special first, then letters,
/// digits, specials and `-`.
pub(crate) fn is_nick(nick: &str) -> bool {
    const SPECIAL: &str = "[]\\`_^{|}";
    let mut characters = nick.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    let tail = |character: char| {
        character.is_ascii_alphanumeric() || character == '-' || SPECIAL.contains(character)
    };
    (first.is_ascii_alphabetic() || SPECIAL.contains(first)) && characters.all(tail)
}

/// A channel name with a standard CHANTYPES prefix and no forbidden
/// characters.
pub(crate) fn is_channel(name: &str) -> bool {
    let mut characters = name.chars();
    let prefixed = characters.next().is_some_and(|first| "#&".contains(first));
    prefixed && name.len() > 1 && !name.contains([' ', ',', '\x07'])
}
