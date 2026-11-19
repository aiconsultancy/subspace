use crate::Target;

const PREFIX: &str = "subspace-search://";
const MAX_LINK: usize = 8192;
const HEX_RADIX: u32 = 16;

/// A local citation with no document content. Invalid/oversize targets yield no link.
pub fn link(target: &Target) -> String {
    let Ok(json) = serde_json::to_vec(target) else {
        return String::new();
    };
    if json.len() * 2 + PREFIX.len() > MAX_LINK {
        return String::new();
    }
    use std::fmt::Write;
    json.iter().fold(PREFIX.into(), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

/// Parse only bounded local citations. Call `Service::read` before using the target.
pub fn parse_link(text: &str) -> Option<Target> {
    if text.len() > MAX_LINK {
        return None;
    }
    let encoded = text.strip_prefix(PREFIX)?;
    if !encoded.len().is_multiple_of(2) {
        return None;
    }
    let bytes: Option<Vec<u8>> = encoded
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let pair = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(pair, HEX_RADIX).ok()
        })
        .collect();
    serde_json::from_slice(&bytes?).ok()
}

#[cfg(test)]
mod tests {
    use crate::{Scope, Target, link, parse_link};

    #[test]
    fn citations_roundtrip_and_reject_malformed_or_oversize_input() {
        let target = Target {
            scope: Scope {
                app: "app".into(),
                board: "board".into(),
                account: "account".into(),
                tenant: String::new(),
                endpoint: "https://example.test".into(),
            },
            id: "東京/123".into(),
        };
        assert_eq!(parse_link(&link(&target)), Some(target.clone()));
        assert!(parse_link("https://example.test").is_none());
        assert!(parse_link("subspace-search://zz").is_none());
        assert!(parse_link("subspace-search://1").is_none());
        assert!(
            link(&Target {
                id: "x".repeat(8192),
                ..target
            })
            .is_empty()
        );
        assert!(parse_link(&format!("subspace-search://{}", "a".repeat(8192))).is_none());
    }
}
