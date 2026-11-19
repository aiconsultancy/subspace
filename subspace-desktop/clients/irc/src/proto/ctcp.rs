//! Parses CTCP frames carried inside IRC messages.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ctcp<'a> {
    pub command: String,
    pub params: &'a str,
}

pub fn parse(text: &str) -> Option<Ctcp<'_>> {
    let body = text.strip_prefix('\x01')?;
    let body = body.strip_suffix('\x01').unwrap_or(body);
    let (command, params) = body.split_once(' ').unwrap_or((body, ""));
    (!command.is_empty()).then(|| Ctcp {
        command: command.to_ascii_uppercase(),
        params,
    })
}
