//! Rich text: service HTML and Markdown, shown with GPUI Kit's `TextView`:
//!
//! - HTML becomes Markdown first ([`crate::html`]): scripts, styles, frames,
//!   forms and similar elements are dropped with their content, and other
//!   tags lose their attributes.
//! - A link opens in the system browser only for `http`, `https` and
//!   `mailto` ([`crate::links::safe_url`]); other links do nothing.
//! - A remote image becomes a `[alt ↗]` link; a `data:image/(png|gif|jpeg|
//!   webp)` image shows. `TextView` would load any image URL, so the source is
//!   rewritten first, and the view's image resolver refuses every source but
//!   those data URLs as a second guard.

use std::sync::Arc;

use base64::Engine as _;
use gpui_kit::base::TextViewStyle;
use gpui_kit::component::menu::ContextMenu;
use gpui_kit::{
    App, ClickEvent, ElementId, Hsla, Image, ImageFormat, ImageSource, InteractiveElement,
    MouseButton, ParentElement, SharedString, Styled, Window,
};

use crate::html::{DROP, Markup, attribute, link_target, markup, skip_element};
use crate::links::{self, safe_url};
use crate::row_menu::MenuChat;

pub(crate) mod markdown_guard;
mod menu;
#[cfg(test)]
mod menu_tests;
#[cfg(test)]
mod payload_tests;
pub(crate) mod plain;
mod view;

pub use plain::PlainText;
pub use view::{RichText, RichTextContext};

/// The markup of a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Markdown,
    Html,
}

/// A plain value shown with the selectable text view, so a property such as
/// `State: Open` can be selected, copied and sent to the chat like the
/// panel's prose. Native selectable text preserves the value exactly,
/// including indentation, line endings and punctuation.
pub fn plain_text(id: impl Into<ElementId>, value: &str) -> PlainText {
    plain_text_with(id, value, MenuChat::Offered)
}

/// [`plain_text`] whose menu offers the chat only when `chat` allows it.
pub fn plain_text_with(id: impl Into<ElementId>, value: &str, chat: MenuChat) -> PlainText {
    PlainText::new(id.into(), value.to_owned().into(), chat)
}

/// Adds the plain-text menu to an existing selectable element. `fallback`
/// must be the safe text the element displays, not its unsanitized source.
// qual:api - legacy selectable hosts share the managed renderer menu policy.
pub fn text_menu<E>(element: E, fallback: impl Into<String>) -> ContextMenu<E>
where
    E: InteractiveElement + ParentElement + Styled,
{
    text_menu_with(element, fallback, MenuChat::Offered)
}

/// [`text_menu`] with its Add to chat entry offered or left out. A chat-free
/// menu holds no selection for the chat either.
pub fn text_menu_with<E>(element: E, fallback: impl Into<String>, chat: MenuChat) -> ContextMenu<E>
where
    E: InteractiveElement + ParentElement + Styled,
{
    menu::with_source(element, menu::Source::Fallback(fallback.into()), chat)
}

fn selected_or_fallback(selected: &str, fallback: &str) -> String {
    if selected.is_empty() {
        fallback.to_string()
    } else {
        selected.to_string()
    }
}

/// Backslash-escapes every ASCII punctuation character, which CommonMark
/// allows for all of them, so nothing in `value` is read as markup.
pub fn escape_markdown(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

/// The rich text element for `source`. `id` must be unique in the view.
pub fn rich_text(id: impl Into<ElementId>, format: Format, source: &str) -> RichText {
    rich_text_with_links(id, format, source, |_, _, _| false)
}

/// Rich text whose links the owner may claim first, such as a chat board's
/// channel and person references. `claim` returns `true` when it handled
/// the URL; other links follow the shared link policy.
pub fn rich_text_with_links(
    id: impl Into<ElementId>,
    format: Format,
    source: &str,
    claim: impl Fn(&str, &mut Window, &mut App) -> bool + Send + Sync + 'static,
) -> RichText {
    let source = match format {
        Format::Markdown => prepare_markdown(source),
        Format::Html => crate::html::to_markdown(source),
    };
    markdown_view(id, source, Arc::new(claim))
}

/// Markdown the caller has already made safe. Skips `prepare_markdown` while
/// retaining selectable text, the shared link policy and the data-image guard.
/// The caller must neutralise raw `<` outside code; `data:` images in the
/// source are still drawn. Domain-specific preparation belongs to the caller;
/// the Issues board prepares issue Markdown before passing it here.
// qual:api - Consumed by app crates through the Framework facade.
pub fn rich_text_prepared(
    id: impl Into<ElementId>,
    source: impl Into<SharedString>,
    claim: impl Fn(&str, &mut Window, &mut App) -> bool + Send + Sync + 'static,
) -> RichText {
    markdown_view(id, source, Arc::new(claim))
}

/// Keep secondary clicks available to the selection menu.
fn activates_link(event: &ClickEvent) -> bool {
    match event {
        ClickEvent::Mouse(click) => {
            matches!(click.up.button, MouseButton::Left | MouseButton::Middle)
        }
        ClickEvent::Keyboard(_) => true,
        ClickEvent::Touch(click) => !click.long_press,
    }
}

/// The colours of the shared rich text: the theme's, with the app's link
/// and code colours. The host adds its heading sizes when it installs this
/// as every text view's default.
pub fn text_view_style(cx: &App) -> TextViewStyle {
    let tokens = crate::Tokens::current(cx);
    TextViewStyle::from_theme(&gpui_kit::base::Theme::global(cx))
        .with_link(tokens.accent)
        .with_code_background(tokens.pane)
}

/// `view` with its body text in `color` instead of the theme's text colour,
/// keeping the shared link and code colours, for muted or warning text. A
/// text view ignores the colour its container sets, as the app leaves Kit's
/// `TextViewDefaults::with_inherit_text_color` off. Headings keep Kit's
/// sizes, so use it for text without headings.
// qual:api
pub fn tinted(view: RichText, color: Hsla, cx: &App) -> RichText {
    view.style(text_view_style(cx).with_foreground(color))
}

fn markdown_view(
    id: impl Into<ElementId>,
    source: impl Into<SharedString>,
    claim: view::Claim,
) -> RichText {
    RichText::new(id.into(), markdown_guard::prepare(source.into()), claim)
}

#[cfg(test)]
#[path = "rich/prepared_tests.rs"]
mod prepared_tests;

/// A search result opens in the app; anything else follows the link policy.
fn follow_link(url: &str, window: &mut Window, cx: &mut App) {
    if let Some(target) = subspace_search::parse_link(url) {
        if let Some(handler) = cx
            .try_global::<crate::SearchNavigation>()
            .map(|handler| handler.0.clone())
        {
            handler(target, window, cx);
        }
    } else {
        links::open(url, cx);
    }
}

/// What the view may show for an image URL: the decoded data URL, or an
/// empty image that loads nothing.
fn image_source(uri: &str) -> ImageSource {
    let image = match data_image(uri) {
        Some((format, bytes)) => Image::from_bytes(format, bytes),
        None => Image::from_bytes(ImageFormat::Png, Vec::new()),
    };
    ImageSource::Image(Arc::new(image))
}

/// Decodes `data:image/(png|gif|jpeg|webp);base64,...`; anything else is
/// `None`.
pub fn data_image(uri: &str) -> Option<(ImageFormat, Vec<u8>)> {
    const DATA_IMAGE_PREFIX: &str = "data:image/";

    let head = uri.get(..DATA_IMAGE_PREFIX.len())?;
    if !head.eq_ignore_ascii_case(DATA_IMAGE_PREFIX) {
        return None;
    }
    let (kind, data) = uri[DATA_IMAGE_PREFIX.len()..].split_once(";base64,")?;
    let format = match kind.to_ascii_lowercase().as_str() {
        "png" => ImageFormat::Png,
        "gif" => ImageFormat::Gif,
        "jpeg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::Webp,
        _ => return None,
    };
    let data: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    Some((format, bytes))
}

/// Rewrites Markdown for display: `![alt](url)` images and inline `<img>`
/// tags follow the image rule, and dropped HTML elements go.
pub fn prepare_markdown(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut rest = markdown;
    enum Next {
        Image(usize),
        Tag(usize),
    }
    loop {
        let next = match (rest.find("!["), rest.find('<')) {
            (Some(at), None) => Next::Image(at),
            (Some(at), Some(tag)) if at < tag => Next::Image(at),
            (_, Some(at)) => Next::Tag(at),
            (None, None) => break,
        };
        match next {
            Next::Image(at) => {
                out.push_str(&rest[..at]);
                match markdown_image(&rest[at..]) {
                    Some((replacement, len)) => {
                        out.push_str(&replacement);
                        rest = &rest[at + len..];
                    }
                    None => {
                        out.push_str("![");
                        rest = &rest[at + 2..];
                    }
                }
            }
            Next::Tag(at) => {
                out.push_str(&rest[..at]);
                let text = &rest[at..];
                match markup(text) {
                    Some(Markup::Tag(tag)) if !tag.closing && DROP.contains(&tag.name.as_str()) => {
                        rest = skip_element(&text[tag.len..], &tag.name);
                    }
                    Some(Markup::Tag(tag)) if tag.name == "img" => {
                        out.push_str(&image_markdown(
                            &attribute(tag.attributes, "src").unwrap_or_default(),
                            &attribute(tag.attributes, "alt").unwrap_or_default(),
                        ));
                        rest = &text[tag.len..];
                    }
                    _ => {
                        out.push('<');
                        rest = &text[1..];
                    }
                }
            }
        }
    }
    out.push_str(rest);
    out
}

/// Reads `![alt](url "title")` at the start of `text`: the replacement and
/// the length read.
fn markdown_image(text: &str) -> Option<(String, usize)> {
    let alt_end = text[2..].find(']')? + 2;
    let alt = &text[2..alt_end];
    let after = text[alt_end + 1..].strip_prefix('(')?;
    let close = after.find(')')?;
    let inside = after[..close].trim();
    let url = inside
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_start_matches('<')
        .trim_end_matches('>');
    Some((image_markdown(url, alt), alt_end + 2 + close + 1))
}

pub(crate) fn image_markdown(src: &str, alt: &str) -> String {
    let alt = match alt.trim() {
        "" => "image",
        alt => alt,
    };
    let alt = alt.replace(['[', ']'], "");
    if data_image(src).is_some() {
        format!("![{alt}]({src})")
    } else if let Some(href) = safe_url(src) {
        format!("[\\[{alt} ↗\\]]({})", link_target(&href))
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        data_image, escape_markdown, image_source, prepare_markdown, selected_or_fallback,
    };
    use crate::html::to_markdown;
    use crate::row_menu::MenuChat;
    use gpui_kit::ImageSource;
    use gpui_kit::base::{TextSelection, TextSelectionLayer};
    use gpui_kit::test::{TestSupportExt as _, TestWindowExt as _};
    use gpui_kit::{
        Context, InteractiveElement as _, IntoElement, Modifiers, MouseButton, ParentElement as _,
        Render, Styled as _, TestAppContext, Window, div, point, px,
    };

    const PIXEL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    #[gpui_kit::test]
    fn the_rich_text_style_uses_the_app_link_and_code_colours(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            let tokens = crate::Tokens::current(cx);
            let style = super::text_view_style(cx);
            assert_eq!(style.link(), tokens.accent);
            assert_eq!(style.code_background(), tokens.pane);
        });
    }

    #[test]
    fn a_plain_value_keeps_its_punctuation() {
        assert_eq!(escape_markdown("Open"), "Open");
        assert_eq!(escape_markdown("src/main.rs:12"), "src\\/main\\.rs\\:12");
        assert_eq!(escape_markdown("*release*"), "\\*release\\*");
        assert_eq!(escape_markdown("a_b \\ c"), "a\\_b \\\\ c");
        assert_eq!(escape_markdown(""), "");
    }

    #[test]
    fn a_multiline_plain_value_keeps_literal_text_for_copy() {
        let value = "GET /api/items?state=open\nsecond line: [ready]";

        assert_eq!(
            escape_markdown(value),
            "GET \\/api\\/items\\?state\\=open\nsecond line\\: \\[ready\\]"
        );
        assert_eq!(selected_or_fallback("", value), value);
    }

    #[test]
    fn text_menu_prefers_a_selection_and_falls_back_to_displayed_text() {
        assert_eq!(
            selected_or_fallback("selected part", "displayed"),
            "selected part"
        );
        assert_eq!(selected_or_fallback("  ", "displayed"), "  ");
    }

    #[test]
    fn html_drops_scripts_attributes_and_unsafe_links() {
        let html = r#"<p onclick="x()">Hi <script>alert(1)</script><b style="color:red">there</b></p><a href="javascript:alert(1)">bad</a> <a href='https://dev.azure.com/a?b=1&amp;c=2'>good</a><!-- note --><style>p{}</style>"#;
        assert_eq!(
            to_markdown(html),
            "Hi **there**\n\nbad [good](https://dev.azure.com/a?b=1&c=2)"
        );
    }

    #[test]
    fn remote_images_become_links_and_data_images_stay() {
        let html = format!(
            r#"<img src="https://evil.example/track.png" alt="Diagram"><img src="{PIXEL}"><img src="file:///etc/x.png"><img src="data:image/svg+xml;base64,PHN2Zz4=">"#
        );
        assert_eq!(
            to_markdown(&html),
            format!("[\\[Diagram ↗\\]](https://evil.example/track.png)![image]({PIXEL})")
        );

        let markdown = format!(
            "See ![shot](https://x.example/a.png \"t\") and ![]({PIXEL}) and <img src=\"http://x.example/b.gif\" alt=\"b\"> and ![bad](javascript:alert)."
        );
        assert_eq!(
            prepare_markdown(&markdown),
            format!(
                "See [\\[shot ↗\\]](https://x.example/a.png) and ![image]({PIXEL}) and [\\[b ↗\\]](http://x.example/b.gif) and ."
            )
        );
        assert_eq!(
            prepare_markdown("a <script>x</script>b <em>c</em>"),
            "a b <em>c</em>"
        );
    }

    #[test]
    fn the_image_resolver_loads_nothing_remote() {
        assert!(data_image(PIXEL).is_some());
        assert!(data_image("https://x.example/a.png").is_none());
        assert!(data_image("data:image/svg+xml;base64,PHN2Zz4=").is_none());
        for uri in ["https://x.example/a.png", "file:///etc/passwd", "a.png"] {
            match image_source(uri) {
                ImageSource::Image(image) => assert!(image.bytes().is_empty(), "{uri}"),
                _ => panic!("{uri} must not resolve to a URI"),
            }
        }
    }

    struct PlainTextTestView;

    impl Render for PlainTextTestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(TextSelectionLayer)
                .child(div().w(px(640.)).h(px(100.)).child(super::plain_text(
                    "plain-text-regression",
                    "\t  leading\ttext\r\nsecond line: [ready] *punctuation*\r\n\tthird",
                )))
        }
    }

    #[gpui_kit::test]
    fn plain_text_selection_preserves_indentation_line_endings_and_punctuation(
        cx: &mut TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_, _| PlainTextTestView);
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });

        cx.simulate_mouse_down(
            point(px(1.), px(12.)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.simulate_mouse_move(
            point(px(620.), px(72.)),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        cx.simulate_mouse_up(
            point(px(620.), px(72.)),
            MouseButton::Left,
            Modifiers::default(),
        );

        cx.update(|window, cx| {
            let _ = window.draw(cx);
            assert_eq!(
                TextSelection::selected_text(window, cx),
                "\t  leading\ttext\r\nsecond line: [ready] *punctuation*\r\n\tthird"
            );
        });
    }

    struct TextMenuFixture(MenuChat);

    impl Render for TextMenuFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(TextSelectionLayer).child(
                div()
                    .id("text-menu-host")
                    .test_support()
                    .w(px(320.))
                    .child(super::plain_text_with("text-menu-value", "SUB-827", self.0)),
            )
        }
    }

    /// The labels of the open context menu's entries, in order.
    fn text_menu_labels(chat: MenuChat, cx: &mut TestAppContext) -> Vec<String> {
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|_, _| TextMenuFixture(chat));
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.right_click("text-menu-host", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            (0usize..4)
                .map_while(|index| window.try_find(index))
                .filter_map(|item| item.label().map(str::to_owned))
                .collect()
        })
    }

    #[gpui_kit::test]
    fn the_text_menu_offers_copy_and_the_chat(cx: &mut TestAppContext) {
        assert_eq!(
            text_menu_labels(MenuChat::Offered, cx),
            ["Copy", "Select all", "Add to chat"]
        );
    }

    #[gpui_kit::test]
    fn a_chat_free_text_menu_offers_copy_only(cx: &mut TestAppContext) {
        assert_eq!(text_menu_labels(MenuChat::Off, cx), ["Copy", "Select all"]);
    }
}

pub(crate) mod native_state;
