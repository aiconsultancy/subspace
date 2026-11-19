//! Credential-like text detection shared by every tool surface.

/// True when text contains a credential marker that must never enter a write preview.
pub fn looks_like_credential(text: &str) -> bool {
    let compact: String = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    MARKERS.iter().any(|marker| compact.contains(marker))
}

const MARKERS: &[&str] = &[
    "github_pat_",
    "ghp_",
    "gho_",
    "ghu_",
    "-----begin",
    "authorization:bearer",
    "authorization:basic",
    "\"token\":",
    "\"accesstoken\":",
    "\"apikey\":",
    "\"api_key\":",
    "\"password\":",
    "access_token=",
    "api_key=",
    "password=",
    "token:",
    "password:",
    "apikey:",
];

#[cfg(test)]
mod tests {
    use super::looks_like_credential;

    #[test]
    fn scan_ignores_case_and_whitespace() {
        assert!(looks_like_credential("Authorization: Bearer value"));
        assert!(looks_like_credential("GITHUB_PAT_example"));
        assert!(!looks_like_credential("ordinary write summary"));
    }
}
