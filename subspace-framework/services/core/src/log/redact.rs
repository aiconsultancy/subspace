//! Masking credentials before a message reaches the log. The patterns err
//! on the side of masking: a word that looks like a secret's name followed
//! by a value loses the value.

use std::sync::LazyLock;

use regex::Regex;

/// What a masked value becomes.
pub const MASK: &str = "***";

/// A pattern and the replacement that keeps its harmless part.
struct Rule {
    pattern: Regex,
    replacement: &'static str,
}

/// The patterns, in the order they apply. A test checks that every one
/// compiles, so a pattern that does not is a test failure, not a quiet gap.
const PATTERNS: [(&str, &str); 8] = [
    // A JWT: three base64url segments, the first starting `{"` encoded.
    (
        r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]+",
        MASK,
    ),
    // GitHub tokens.
    (r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}\b", MASK),
    (r"\bgithub_pat_[A-Za-z0-9_]{20,}\b", MASK),
    // An Authorization header value, keeping the scheme.
    (
        r#"(?i)\b(authorization\s*[:=]\s*"?)((?:bearer|basic|negotiate|token)\s+)?[^\s"',;]+"#,
        "${1}${2}***",
    ),
    // A bare `Bearer x` or `Basic x`.
    (
        r"(?i)\b(bearer|basic)\s+[A-Za-z0-9\-._~+/]{8,}=*",
        "${1} ***",
    ),
    // Query parameters that carry a credential.
    (
        r"(?i)([?&](?:sig|signature|token|access_token|refresh_token|id_token|code|client_secret|key|api_key|apikey|sas|password|pwd|secret)=)[^&\s\x22']+",
        "${1}***",
    ),
    // `token: x`, `"client_secret": "x"`, `x-api-key=x`, `EXAMPLE_PAT=x`.
    (
        r#"(?i)\b([a-z0-9_-]*(?:token|secret|password|passwd|pwd|pat|api[_-]?key|apikey|private[_-]?key|signature)"?\s*[:=]\s*"?)([^\s"',;}&]+)"#,
        "${1}***",
    ),
    // A long secret-looking value after a secret-looking word.
    (
        r"(?i)\b((?:secret|key|token)\s+)[A-Za-z0-9+/=_-]{32,}",
        "${1}***",
    ),
];

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    PATTERNS
        .iter()
        .filter_map(|(pattern, replacement)| {
            Some(Rule {
                pattern: Regex::new(pattern).ok()?,
                replacement,
            })
        })
        .collect()
});

/// `text` with every credential it holds masked.
pub fn redact(text: &str) -> String {
    RULES.iter().fold(text.to_owned(), |text, rule| {
        rule.pattern
            .replace_all(&text, rule.replacement)
            .into_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::{PATTERNS, RULES, redact};
    use base64::Engine;

    #[test]
    fn every_pattern_compiles() {
        assert_eq!(RULES.len(), PATTERNS.len());
    }

    #[test]
    fn credentials_are_masked_wherever_they_appear() {
        // Synthetic inputs exercise redaction without real credentials.
        let basic = format!(
            "authorization=Basic {}",
            base64::engine::general_purpose::STANDARD.encode("test-user:not-a-real-password")
        );
        let cases = [
            (
                concat!("Authorization: Bear", "er abcdefghijklmnop.qrstuvwxyz"),
                "Authorization: Bearer ***",
            ),
            (basic.as_str(), "authorization=Basic ***"),
            (
                "header Authorization: secret-value-1",
                "header Authorization: ***",
            ),
            (
                "sent bearer abcdefghijklmnopqrstuvwxyz0123",
                "sent bearer ***",
            ),
            (
                concat!(
                    "jwt eyJhbGciOiJSUzI1NiIs",
                    ".eyJzdWIiOiIxMjM0NTY3ODkwIn0",
                    ".c2lnbmF0dXJlLXNpZ25hdHVyZQ end"
                ),
                "jwt *** end",
            ),
            (
                "https://host/path?sv=1&sig=abc%2Fdef&se=2026",
                "https://host/path?sv=1&sig=***&se=2026",
            ),
            (
                "GET /x?api-version=1&token=abc123",
                "GET /x?api-version=1&token=***",
            ),
            ("ghp_abcdefghijklmnopqrstuvwxyz0123456789", "***"),
            ("github_pat_11ABCDEFG0123456789_abcdefghij", "***"),
            (
                r#"{"client_secret": "s3cr3t-value", "name": "x"}"#,
                r#"{"client_secret": "***", "name": "x"}"#,
            ),
            ("EXAMPLE_PAT=abcdef0123456789", "EXAMPLE_PAT=***"),
            ("x-api-key: k-123456", "x-api-key: ***"),
            ("password=hunter2;", "password=***;"),
            ("token: abc, then more", "token: ***, then more"),
        ];
        for (input, expected) in cases {
            assert_eq!(redact(input), expected, "input: {input}");
        }
    }

    #[test]
    fn ordinary_text_paths_and_ids_survive() {
        let cases = [
            "GET management.azure.com /subscriptions/3cd6d1b7-8d08-4a22-bb62-aa171fa9d38b/providers/Microsoft.CostManagement/query 200 843ms",
            "Cost Management answered 429 after 3 retries",
            "https://portal.azure.com/#@/resource/subscriptions/x/resourceGroups/rg/overview",
            "status=200 duration_ms=12 host=api.github.com",
            "Reading the group's resources; no token reaches the log",
            "format=json path=/tmp/x.csv",
        ];
        for input in cases {
            assert_eq!(redact(input), input);
        }
    }
}
