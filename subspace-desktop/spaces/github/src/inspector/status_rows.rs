//! The inspector's status rows: a pull request's reviewers and its checks,
//! each with the glyph that tells its status by shape.

use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div,
};
use subspace_framework::client_github::model::GhItemDetail;
use subspace_framework::dashboard::workspace;
use subspace_framework::dashboard::{Tokens, avatar, links, truncated};

use crate::peek::{self, StatusRow};

/// Reviewers: one row per reviewer with its status, "…" until the details
/// load, or "None".
pub(super) fn reviewers(detail: Option<&GhItemDetail>, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let section = workspace::section("Reviewers", cx);
    let section = match detail {
        None => section.child(quiet("…", &tokens)),
        Some(detail) if detail.reviewers.is_empty() => section.child(quiet("None", &tokens)),
        Some(detail) => section.children(
            peek::reviewer_rows(detail)
                .into_iter()
                .enumerate()
                .map(|(index, row)| status_row("gh-reviewer", index, row, true, &tokens)),
        ),
    };
    section.into_any_element()
}

fn quiet(text: &'static str, tokens: &Tokens) -> Div {
    div()
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child(text)
}

/// A reviewer's or a check's row: the status glyph, which tells the status
/// by its shape, an avatar for a person, the name, the status word, and a
/// link to the check's page when it has one.
pub(super) fn status_row(
    prefix: &'static str,
    index: usize,
    row: StatusRow,
    person: bool,
    tokens: &Tokens,
) -> AnyElement {
    let colour = tokens.glyph(&row.glyph);
    let url = row.url.and_then(|url| links::safe_url(&url).map(|_| url));
    let name_id = SharedString::from(format!("{prefix}-name:{index}"));
    let name = row.name.clone();
    h_flex()
        .id((prefix, index))
        .aria_label(format!("{}: {}", row.name, row.word))
        .w_full()
        .min_w_0()
        .gap_2()
        .text_size(tokens.secondary_size())
        .child(
            div()
                .flex_shrink_0()
                .text_color(colour)
                .child(row.glyph.text.clone()),
        )
        .when(person, |line| {
            line.child(avatar::member(row.name.clone(), tokens))
        })
        .child(
            truncated::text(name_id, row.name)
                .flex_1()
                .text_color(tokens.fg_strong),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(tokens.muted)
                .child(row.word),
        )
        .when_some(url, |line, url| {
            line.child(
                div()
                    .id(("gh-check-link", index))
                    .aria_label(link_label(&name, &url))
                    .cursor_pointer()
                    .text_color(tokens.accent)
                    .child("↗")
                    .on_click(move |_, _, cx| {
                        links::open(&url, cx);
                    }),
            )
        })
        .into_any_element()
}

/// The spoken name of a check's link: the check, and the site it opens,
/// since a check's page is often another CI service's rather than GitHub's.
pub(crate) fn link_label(name: &str, url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    // A bracketed IPv6 host holds colons of its own: the port follows the
    // closing bracket.
    let host = if host.starts_with('[') {
        host.find(']').map_or(host, |end| &host[..=end])
    } else {
        host.split(':').next().unwrap_or_default()
    }
    .to_lowercase();
    if host.is_empty() {
        return format!("Open the {name} check's page");
    }
    let github = host == "github.com" || host.ends_with(".github.com");
    let site = if github { "GitHub" } else { host.as_str() };
    format!("Open the {name} check on {site}")
}

#[cfg(test)]
mod tests {
    use super::link_label;

    #[test]
    fn a_check_link_names_the_site_it_opens() {
        assert_eq!(
            link_label("build", "https://github.com/octo-org/api/actions/runs/7"),
            "Open the build check on GitHub"
        );
        assert_eq!(
            link_label("deploy", "https://ci.example.com:8443/jobs/12?tab=log"),
            "Open the deploy check on ci.example.com"
        );
        assert_eq!(
            link_label("lint", "https://bot@CI.Example.org/run/3"),
            "Open the lint check on ci.example.org"
        );
    }

    #[test]
    fn a_check_link_on_an_ipv6_host_names_the_whole_address() {
        assert_eq!(
            link_label("build", "http://[2001:DB8::7]:8080/runs/7"),
            "Open the build check on [2001:db8::7]"
        );
        assert_eq!(
            link_label("lint", "https://bot@[::1]/run/3"),
            "Open the lint check on [::1]"
        );
    }
}
