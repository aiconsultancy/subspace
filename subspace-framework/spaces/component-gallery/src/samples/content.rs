//! Content: media previews, badges, links, shortened and rich text, headings,
//! toolbar labels and avatars.

use gpui_kit::{
    Context, InteractiveElement as _, ParentElement as _, Styled as _, Window, div, px,
};
use subspace_dashboard::activity_mark::ActivityMark;
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
#[cfg(test)]
pub(super) use links::{external, external_marked, external_marked_with_label};
pub(super) use reference::{reference_cards, reference_frames};

const SHORT_WIDTH: f32 = 180.;
const RICH_WIDTH: f32 = 360.;
const LONG_TEXT: &str =
    "A long fixture title that does not fit in the narrow sample box and is shortened";
const MARKDOWN: &str = "**Bold**, *italic*, `code` and a [link](https://example.com/component-gallery).\n\n- First point\n- Second point";
const HTML: &str = "<p><b>Bold</b> HTML with <code>code</code>.</p>";

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "activity_mark::ActivityMark",
        source: "board-kit/dashboard/src/activity_mark.rs",
        used_in: "Active agent turns in Channels and the Subspace toolbar while the current Space is busy.",
        samples: activity_marks,
        kit: None,
    },
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
        used_in: "Channels issue and pull request cards, chips and status pills.",
        samples: reference_cards,
        kit: None,
    },
    Item {
        name: "reference_card::frame",
        source: "board-kit/dashboard/src/reference_card.rs",
        used_in: "Channels issue and pull request hover cards with a neutral outline and coloured status pills.",
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
    links::EXTERNAL,
    links::MARKED,
    links::MARKED_WITH_LABEL,
    Item {
        name: "truncated::text",
        source: "board-kit/dashboard/src/truncated.rs",
        used_in: "Shortened titles with the full text on hover in Space Lab, GitHub, chat and the host.",
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
        "Right-click for Copy, Select all and Add to local model chat when Chat is enabled",
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
        sample(
            "Right-click anywhere in the row for its action and text menu",
            sized(
                RICH_WIDTH,
                rich::rich_text(
                    "gallery-sample:rich-row",
                    Format::Markdown,
                    "A selectable message.",
                )
                .menu_host(|text| {
                    div()
                        .id("gallery-sample:rich-menu-row")
                        .w_full()
                        .px_3()
                        .py_2()
                        .child(text)
                })
                .menu_action(|| {
                    gpui_kit::component::menu::PopupMenuItem::new("Reply in thread")
                        .icon(subspace_dashboard::panel::IconName::Reply)
                }),
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
        "Find a **needle** here. Right-click the [fixture link](https://example.com/component-gallery) for its text menu.",
    );
    let plain = rich::plain_text("gallery-find-literal", "Select this plain-text needle.");
    let content = gpui_kit::component::v_flex().gap_3().p_3();
    let content = content.child(open).child(plain).child(text);
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
    let label = sample_label(
        Some(IconName::User),
        "octo-dev",
        "Signed in to GitHub as octo-dev",
    );
    let plain = sample_label(None, "Grouped by state", "Grouped by state");
    vec![
        sample(
            "One account",
            div().bg(tokens.bg).child(toolbar_label::label(
                "gallery-sample:toolbar-label",
                label,
                cx,
            )),
        ),
        sample(
            "Plain quiet text, without a glyph",
            div().bg(tokens.bg).child(toolbar_label::label(
                "gallery-sample:toolbar-label-plain",
                plain,
                cx,
            )),
        ),
    ]
}

/// A toolbar label sample: `text`, after `icon` when it has one, spoken as
/// `accessible`.
fn sample_label(
    icon: Option<IconName>,
    text: &'static str,
    accessible: &'static str,
) -> ToolbarLabel {
    ToolbarLabel {
        icon: icon.map(Into::into),
        text: text.into(),
        accessible: accessible.into(),
    }
}

pub(super) fn activity_marks(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "Working",
            ActivityMark::new("gallery-activity-running", px(20.)),
        ),
        sample(
            "Steady",
            ActivityMark::new("gallery-activity-steady", px(20.)).active(false),
        ),
    ]
}
