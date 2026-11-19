//! Writes messages in wire order without the line terminator.

use super::{Message, Source};

pub fn serialise(message: &Message) -> String {
    let mut wire = String::new();
    write_tags(&mut wire, message);
    if let Some(source) = &message.source {
        write_source(&mut wire, source);
    }
    wire.push_str(&message.command);
    write_params(&mut wire, &message.params);
    wire
}

fn write_tags(wire: &mut String, message: &Message) {
    for (index, (key, value)) in message.tags.iter().enumerate() {
        wire.push(if index == 0 { '@' } else { ';' });
        wire.push_str(key);
        if !value.is_empty() {
            wire.push('=');
            escape(wire, value);
        }
    }
    if !message.tags.is_empty() {
        wire.push(' ');
    }
}

fn write_params(wire: &mut String, params: &[String]) {
    if let Some((last, middle)) = params.split_last() {
        for param in middle {
            wire.push(' ');
            wire.push_str(param);
        }
        wire.push(' ');
        if last.is_empty() || last.contains(' ') || last.starts_with(':') {
            wire.push(':');
        }
        wire.push_str(last);
    }
}

fn write_source(wire: &mut String, source: &Source) {
    wire.push(':');
    match source {
        Source::Server(name) => wire.push_str(name),
        Source::User { nick, user, host } => {
            wire.push_str(nick);
            if let Some(user) = user {
                wire.push('!');
                wire.push_str(user);
            }
            if let Some(host) = host {
                wire.push('@');
                wire.push_str(host);
            }
        }
    }
    wire.push(' ');
}

fn escape(wire: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '\\' => wire.push_str(r"\\"),
            ';' => wire.push_str(r"\:"),
            ' ' => wire.push_str(r"\s"),
            '\r' => wire.push_str(r"\r"),
            '\n' => wire.push_str(r"\n"),
            other => wire.push(other),
        }
    }
}
