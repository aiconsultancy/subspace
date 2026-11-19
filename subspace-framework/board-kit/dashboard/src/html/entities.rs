/// Decodes character references: common named references and numeric values.
pub(crate) fn decode_entities(text: &str) -> String {
    const MAX_REFERENCE_NAME: usize = 10;
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let decoded = rest
            .find(';')
            .filter(|end| *end <= MAX_REFERENCE_NAME)
            .and_then(|end| decode_reference(&rest[1..end]).map(|c| (c, end + 1)));
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_reference(name: &str) -> Option<char> {
    let named = match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        "ndash" => Some('–'),
        "mdash" => Some('—'),
        "hellip" => Some('…'),
        "rsquo" => Some('’'),
        "lsquo" => Some('‘'),
        "rdquo" => Some('”'),
        "ldquo" => Some('“'),
        "bull" => Some('•'),
        "rarr" => Some('→'),
        _ => None,
    };
    named.or_else(|| {
        let number = name.strip_prefix('#')?;
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        char::from_u32(code)
    })
}
