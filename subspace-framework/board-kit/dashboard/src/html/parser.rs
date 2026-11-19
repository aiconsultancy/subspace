pub(crate) struct Tag<'a> {
    pub name: String,
    pub closing: bool,
    pub attributes: &'a str,
    pub len: usize,
}

pub(crate) enum Markup<'a> {
    Tag(Tag<'a>),
    Skip(usize),
}

pub(crate) fn markup(text: &str) -> Option<Markup<'_>> {
    const COMMENT_PREFIX_LEN: usize = 4;
    const COMMENT_SUFFIX_LEN: usize = 3;
    if let Some(rest) = text.strip_prefix("<!--") {
        let end = rest.find("-->").map_or(text.len(), |at| {
            at + COMMENT_PREFIX_LEN + COMMENT_SUFFIX_LEN
        });
        return Some(Markup::Skip(end));
    }
    if text.starts_with("<!") || text.starts_with("<?") {
        return Some(Markup::Skip(text.find('>').map_or(text.len(), |at| at + 1)));
    }
    let body = &text[1..];
    let (closing, body) = body
        .strip_prefix('/')
        .map_or((false, body), |rest| (true, rest));
    let name_len = body
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(body.len());
    if name_len == 0 || !body.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }
    let after_name = &body[name_len..];
    let end = tag_end(after_name)?;
    let prefix = text.len() - after_name.len();
    Some(Markup::Tag(Tag {
        name: body[..name_len].to_ascii_lowercase(),
        closing,
        attributes: &after_name[..end],
        len: prefix + end + 1,
    }))
}

fn tag_end(text: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in text.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

pub(crate) fn attribute(attributes: &str, wanted: &str) -> Option<String> {
    let mut rest = attributes;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        if rest.is_empty() {
            return None;
        }
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '=' || c == '/')
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        rest = rest[name_end..].trim_start();
        let (value, next) = attribute_value(rest);
        if name.eq_ignore_ascii_case(wanted) {
            return Some(value);
        }
        rest = next;
    }
}

fn attribute_value(rest: &str) -> (String, &str) {
    let Some(after) = rest.strip_prefix('=') else {
        return (String::new(), rest);
    };
    let after = after.trim_start();
    let (raw, next) = match after.chars().next() {
        Some(q @ ('"' | '\'')) => {
            let inner = &after[1..];
            let close = inner.find(q).unwrap_or(inner.len());
            (&inner[..close], inner.get(close + 1..).unwrap_or(""))
        }
        _ => {
            let close = after.find(char::is_whitespace).unwrap_or(after.len());
            (&after[..close], &after[close..])
        }
    };
    (super::decode_entities(raw), next)
}

pub(crate) fn skip_element<'a>(text: &'a str, name: &str) -> &'a str {
    let lower = text.to_ascii_lowercase();
    let close = format!("</{name}");
    match lower.find(&close) {
        Some(at) => text[at..].find('>').map_or("", |end| &text[at + end + 1..]),
        None => "",
    }
}
