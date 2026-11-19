use crate::Issues;
use gpui_kit::base::{
    MarkdownExtensions, MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast::Node,
};
use gpui_kit::{App, IntoElement, WeakEntity, Window};

pub(crate) fn install(
    text: subspace_framework::dashboard::rich::RichText,
    owner: &WeakEntity<Issues>,
) -> subspace_framework::dashboard::rich::RichText {
    let owner = owner.clone();
    text.native(move |text, _| {
        text.markdown_extensions(MarkdownExtensions::default().plugin(MediaPlugin(owner)))
    })
}

struct MediaPlugin(WeakEntity<Issues>);

#[derive(Clone)]
struct MediaNode {
    id: String,
    label: String,
    image: bool,
}

impl MarkdownPlugin for MediaPlugin {
    fn name(&self) -> &str {
        "issues-media"
    }
    fn parse(&self, node: &Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Link(link) = node else {
            return None;
        };
        let (id, image) = if let Some(id) = link.url.strip_prefix("subspace-media:") {
            (id, true)
        } else {
            (link.url.strip_prefix("subspace-file:")?, false)
        };
        let label: String = link.children.iter().map(ToString::to_string).collect();
        Some(
            MarkdownNode::new(
                self.name(),
                MediaNode {
                    id: id.into(),
                    label: label.clone(),
                    image,
                },
            )
            .text(label),
        )
    }
    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(media) = node.data::<MediaNode>() else {
            return gpui_kit::div().into_any_element();
        };
        if !media.image {
            return self
                .0
                .update(cx, |issues, cx| file(issues, media, cx))
                .unwrap_or_else(|_| super::view::file_chip(&media.label, cx).into_any_element());
        }
        self.0
            .update(cx, |issues, cx| {
                let descriptor = super::view::descriptor(issues, &media.id);
                match descriptor {
                    Some(descriptor) => {
                        super::view::tile(issues, descriptor, &media.label, true, cx)
                    }
                    None => gpui_kit::div()
                        .child("Image unavailable")
                        .into_any_element(),
                }
            })
            .unwrap_or_else(|_| gpui_kit::div().into_any_element())
    }
}

/// A text or Markdown reference opens the attachment panel, as its
/// Attachments row does; other files stay read-only chips.
fn file(
    issues: &Issues,
    media: &MediaNode,
    cx: &mut gpui_kit::Context<Issues>,
) -> gpui_kit::AnyElement {
    match super::view::descriptor(issues, &media.id).filter(super::text::viewable) {
        Some(descriptor) => super::files::open_button(
            descriptor,
            media.label.clone(),
            "attachment-inline-open",
            cx,
        ),
        None => super::view::file_chip(&media.label, cx).into_any_element(),
    }
}

use gpui_kit::ParentElement as _;
