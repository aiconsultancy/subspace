//! Content: media previews, badges, links, shortened and rich text, headings,
//! toolbar labels and avatars.

use gpui_kit::{Context, ParentElement as _, Styled as _, Window, div};
use subspace_dashboard::rich::{self, Format};
use subspace_dashboard::toolbar_label::{self, ToolbarLabel};
use subspace_dashboard::{Tokens, avatar, truncated, typography};
use subspace_lucide::IconName;

pub(super) use self::media::{attachment_thumbnails, file_chip, media_tile};
use super::{Sample, sample, sized};
use crate::ComponentGallery;
use crate::catalog::Item;

mod badges;
pub(super) mod disclosure;
mod links;
mod media;
mod reference;
pub(super) use badges::badges;
pub(crate) use badges::pills;
pub(super) use links::{external, external_marked};
pub(super) use reference::{reference_cards, reference_frames};

const SHORT_WIDTH: f32 = 180.;
const RICH_WIDTH: f32 = 360.;
const LONG_TEXT: &str =
    "A long fixture title that does not fit in the narrow sample box and is shortened";
const MARKDOWN: &str = "**Bold**, *italic*, `code` and a [link](https://example.com/component-gallery).\n\n- First point\n- Second point";
const HTML: &str = "<p><b>Bold</b> HTML with <code>code</code>.</p>";

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "animated_collapsible::AnimatedCollapsible",
        source: "board-kit/dashboard/src/animated_collapsible.rs",
        used_in: "Channels summaries: measured height and opacity, reversible over 200 ms with reduced motion support.",
        samples: disclosure::samples,
        kit: None,
    },
    Item {
        name: "reference_card::surface",
        source: "board-kit/dashboard/src/reference_card.rs",
        used_in: "Channels issue chips, status pills and assignee avatars.",
        samples: reference_cards,
        kit: None,
    },
    Item {
        name: "reference_card::frame",
        source: "board-kit/dashboard/src/reference_card.rs",
        used_in: "Passive Channels issue and tracker hover cards, with shared popover elevation.",
        samples: reference_frames,
        kit: None,
    },
    Item {
        name: "media::tile",
        source: "board-kit/dashboard/src/media.rs",
        used_in: "Private image previews in Issues, with native Open and Retry actions.",
        samples: media_tile,
        kit: None,
    },
    Item {
        name: "media::file_chip",
        source: "board-kit/dashboard/src/media.rs",
        used_in: "File metadata in Issues descriptions and comments.",
        samples: file_chip,
        kit: None,
    },
    Item {
        name: "media::thumbnail",
        source: "board-kit/dashboard/src/media.rs",
        used_in: "Attachment thumbnails in a message, and the unavailable chip for missing or expired attachments.",
        samples: attachment_thumbnails,
        kit: None,
    },
    Item {
        name: "badge::badge",
        source: "board-kit/dashboard/src/badge.rs",
        used_in: "Host switcher tab counts and menu digit hints; no board uses it yet.",
        samples: badges,
        kit: None,
    },
    Item {
        name: "badge::Pill",
        source: "board-kit/dashboard/src/badge/pill.rs",
        used_in: "Channels composer draft limits.",
        samples: pills,
        kit: None,
    },
    Item {
        name: "links::external",
        source: "board-kit/dashboard/src/links.rs",
        used_in: "External links in the host.",
        samples: external,
        kit: None,
    },
    Item {
        name: "links::external_marked",
        source: "board-kit/dashboard/src/links.rs",
        used_in: "External links with a trailing mark; no board uses it at present.",
        samples: external_marked,
        kit: None,
    },
    Item {
        name: "truncated::text",
        source: "board-kit/dashboard/src/truncated.rs",
        used_in: "Shortened titles with the full text on hover in Board Lab, GitHub, chat and the host.",
        samples: truncated_text,
        kit: None,
    },
    Item {
        name: "rich::plain_text",
        source: "board-kit/dashboard/src/rich.rs",
        used_in: "Selectable plain text with a copy menu in Actions, GitHub, IRC and the host.",
        samples: plain_text,
        kit: None,
    },
    Item {
        name: "rich::rich_text",
        source: "board-kit/dashboard/src/rich.rs",
        used_in: "Markdown and HTML bodies in GitHub and the host.",
        samples: rich_text,
        kit: None,
    },
    Item {
        name: "find_in_page::surface",
        source: "board-kit/dashboard/src/find_in_page.rs",
        used_in: "Local text find in each Space and Help, with native selection retained.",
        samples: find_in_page,
        kit: None,
    },
    Item {
        name: "typography::group_heading",
        source: "board-kit/dashboard/src/typography.rs",
        used_in: "Group header titles in GitHub and the host.",
        samples: group_heading,
        kit: None,
    },
    Item {
        name: "toolbar_label::label",
        source: "board-kit/dashboard/src/toolbar_label.rs",
        used_in: "A toolbar fact with nothing to choose: the GitHub Space's account when only one is signed in.",
        samples: toolbar_labels,
        kit: None,
    },
    Item {
        name: "avatar::person",
        source: "board-kit/dashboard/src/avatar.rs",
        used_in: "Chat member avatars; no board uses it at present.",
        samples: person,
        kit: None,
    },
    Item {
        name: "avatar::group",
        source: "board-kit/dashboard/src/avatar.rs",
        used_in: "Group chat avatars; no board uses it at present.",
        samples: group,
        kit: None,
    },
];

pub(super) fn truncated_text(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "Shortened",
            sized(
                SHORT_WIDTH,
                truncated::text("gallery-sample:truncated", LONG_TEXT),
            ),
        ),
        sample(
            "Wrapped",
            sized(
                SHORT_WIDTH,
                truncated::text("gallery-sample:truncated-wrap", LONG_TEXT).wrap(true),
            ),
        ),
    ]
}

pub(super) fn plain_text(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Select text or right-click for Copy, Select all and Add to chat",
        rich::plain_text("gallery-sample:plain-text", "Select this fixture text."),
    )]
}

pub(super) fn rich_text(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "Markdown",
            sized(
                RICH_WIDTH,
                rich::rich_text("gallery-sample:rich-markdown", Format::Markdown, MARKDOWN),
            ),
        ),
        sample(
            "HTML",
            sized(
                RICH_WIDTH,
                rich::rich_text("gallery-sample:rich-html", Format::Html, HTML),
            ),
        ),
    ]
}

pub(super) fn find_in_page(
    _: &ComponentGallery,
    tokens: &Tokens,
    window: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    use subspace_dashboard::action::{Intent, action_button};
    let open = action_button(
        "gallery-find-open",
        Intent::Quiet,
        Some(IconName::Search),
        "Find in sample",
    )
    .focus_on_click()
    .on_click(|_, window, cx| {
        window.dispatch_action(Box::new(subspace_dashboard::find_in_page::Open), cx)
    });
    let text = rich::rich_text(
        "gallery-find-text",
        Format::Markdown,
        "Find a **needle** here, and a second NEEDLE below. Right-click a [link](https://example.com/component-gallery) or select part of this block.",
    );
    let content = gpui_kit::component::v_flex()
        .gap_3()
        .p_3()
        .child(open)
        .child(text);
    let content = subspace_dashboard::find_in_page::surface("gallery-find", content, window, cx);
    vec![sample(
        "Local find and text menu",
        super::bounded((520., 240.), tokens, content),
    )]
}

pub(super) fn group_heading(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Default",
        typography::group_heading(div().child("Group heading"), cx),
    )]
}

pub(super) fn person(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample("Initials", avatar::person("Ada Lovelace", None))]
}

pub(super) fn group(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let people = |names: &[&'static str]| {
        avatar::group(names.iter().map(|name| avatar::person(*name, None)))
    };
    vec![
        sample("One person", people(&["Ada Lovelace"])),
        sample("Two people", people(&["Ada Lovelace", "Grace Hopper"])),
        sample(
            "Three people (limit 2)",
            people(&["Ada Lovelace", "Grace Hopper", "Alan Turing"]),
        ),
    ]
}

pub(super) fn toolbar_labels(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let label = ToolbarLabel {
        icon: IconName::User.into(),
        text: "octo-dev".into(),
        accessible: "Signed in to GitHub as octo-dev".into(),
    };
    vec![sample(
        "One account",
        div().bg(tokens.bg).child(toolbar_label::label(
            "gallery-sample:toolbar-label",
            label,
            cx,
        )),
    )]
}
