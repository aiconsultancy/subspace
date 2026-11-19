use serde::Serialize;

use super::ItemKind;

/// One gh-dash-style section: a saved GitHub search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SectionDef {
    /// Stable, URL-safe.
    pub id: &'static str,
    pub kind: ItemKind,
    pub title: &'static str,
    /// GitHub search query; `@me` resolves to the account.
    pub query: &'static str,
    /// The section searches only the repositories it is given, and is empty
    /// without any: it has no account qualifier of its own.
    pub repos_only: bool,
}

const fn section(
    id: &'static str,
    kind: ItemKind,
    title: &'static str,
    query: &'static str,
) -> SectionDef {
    SectionDef {
        id,
        kind,
        title,
        query,
        repos_only: false,
    }
}

const fn repos_section(
    id: &'static str,
    kind: ItemKind,
    title: &'static str,
    query: &'static str,
) -> SectionDef {
    SectionDef {
        repos_only: true,
        ..section(id, kind, title, query)
    }
}

/// The gh-dash-style sections (GH-03).
pub const SECTIONS: &[SectionDef] = &[
    section(
        "mine",
        ItemKind::Pr,
        "Mine",
        "is:pr is:open author:@me archived:false",
    ),
    section(
        "review",
        ItemKind::Pr,
        "Review requested",
        "is:pr is:open review-requested:@me archived:false",
    ),
    section(
        "involved",
        ItemKind::Pr,
        "Involved",
        "is:pr is:open involves:@me -author:@me archived:false",
    ),
    repos_section(
        "open",
        ItemKind::Pr,
        "All open",
        "is:pr is:open archived:false",
    ),
    section(
        "assigned",
        ItemKind::Issue,
        "Assigned",
        "is:issue is:open assignee:@me archived:false",
    ),
    section(
        "created",
        ItemKind::Issue,
        "Created",
        "is:issue is:open author:@me archived:false",
    ),
    section(
        "mentioned",
        ItemKind::Issue,
        "Mentioned",
        "is:issue is:open mentions:@me archived:false",
    ),
];

/// `GET /api/gh/image`: an image's bytes, up to 10 MB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub content_type: &'static str,
    pub bytes: Vec<u8>,
}

/// The image `Content-Type` for a repository path, by its extension.
pub fn image_type(path: &str) -> Option<&'static str> {
    let (_, extension) = path.rsplit_once('.')?;
    if extension.is_empty() || extension.contains('/') {
        return None;
    }
    Some(match extension.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

/// EMU logins end in the enterprise shortcode.
pub fn short_login(login: &str) -> &str {
    match login.rsplit_once('_') {
        Some((head, code))
            if !code.is_empty() && code.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            head
        }
        _ => login,
    }
}

/// The name, else the short login.
pub fn display_name(user: &super::GhUser) -> &str {
    user.name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| short_login(&user.login))
}

#[cfg(test)]
mod tests {
    use super::{ItemKind, SECTIONS, repos_section, section};

    #[test]
    fn only_all_open_is_limited_to_repositories() {
        let open = repos_section("open", ItemKind::Pr, "All open", "is:pr is:open");
        assert!(open.repos_only);
        assert!(!section("mine", ItemKind::Pr, "Mine", "is:pr").repos_only);
        let scoped: Vec<&str> = SECTIONS
            .iter()
            .filter(|section| section.repos_only)
            .map(|section| section.id)
            .collect();
        assert_eq!(scoped, ["open"]);
    }
}
