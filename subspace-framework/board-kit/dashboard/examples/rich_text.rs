//! Rich text samples in one window, to check `rich::rich_text` by eye:
//! `cargo run -p subspace-dashboard --example rich_text [file.html]`.
//! With a file, the window shows that HTML in a scrolling panel body.

use gpui_kit::component::v_flex;
use gpui_kit::{
    AppContext, Context, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, WindowBounds, WindowOptions, div, px, rgb, size,
};
use subspace_dashboard::rich::{Format, rich_text};

/// Page background, matching the One Dark editor background.
const BACKGROUND_RGB: u32 = 0x15171b;
/// Default body text colour.
const TEXT_RGB: u32 = 0xabb2bf;
/// Colour of the sample name above each sample.
const SAMPLE_LABEL_RGB: u32 = 0x7dd3fc;
const BODY_TEXT_SIZE_PX: f32 = 13.;
const BODY_WIDTH_PX: f32 = 600.;
const BODY_TOP_PADDING_PX: f32 = 16.;
const BODY_SIDE_PADDING_PX: f32 = 20.;
const WINDOW_WIDTH_PX: f32 = 700.;
const WINDOW_HEIGHT_PX: f32 = 800.;

const SAMPLES: &[(&str, Format, &str)] = &[
    (
        "html-code-first",
        Format::Html,
        "<h3>Outcome </h3><p><code>Eva.Job.Handler</code> reconciles Jobs. </p>",
    ),
    (
        "html-code-inside",
        Format::Html,
        "<p>The observer inside <code>Eva.Job.Handler</code> reconciles Kubernetes Jobs and Pods into deterministic lifecycle transitions without durable producer state. </p>",
    ),
    (
        "html-list-code",
        Format::Html,
        "<ul><li>Map them to <code>queued</code>, <code>running</code>, or <code>failed</code>. </li><li>Plain item. </li> </ul>",
    ),
    (
        "markdown-code",
        Format::Markdown,
        "`Eva.Job.Handler` reconciles Jobs and `Pods` into transitions.",
    ),
];

struct Gallery {
    file: Option<String>,
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let body = match &self.file {
            Some(html) => v_flex().child(rich_text("file", Format::Html, html)),
            None => v_flex()
                .gap_4()
                .children(SAMPLES.iter().map(|(id, format, source)| {
                    v_flex()
                        .gap_1()
                        .child(div().text_color(rgb(SAMPLE_LABEL_RGB)).child(*id))
                        .child(rich_text(*id, *format, source))
                })),
        };
        div()
            .size_full()
            .bg(rgb(BACKGROUND_RGB))
            .text_color(rgb(TEXT_RGB))
            .text_size(px(BODY_TEXT_SIZE_PX))
            .child(
                div()
                    .id("body")
                    .w(px(BODY_WIDTH_PX))
                    .h_full()
                    .overflow_y_scroll()
                    .pt(px(BODY_TOP_PADDING_PX))
                    .px(px(BODY_SIDE_PADDING_PX))
                    .child(body),
            )
    }
}

fn main() -> std::io::Result<()> {
    let file = std::env::args()
        .nth(1)
        .map(std::fs::read_to_string)
        .transpose()?;
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(
                    size(px(WINDOW_WIDTH_PX), px(WINDOW_HEIGHT_PX)),
                    cx,
                )),
                ..Default::default()
            };
            if let Err(error) =
                gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| Gallery { file }))
            {
                eprintln!("rich_text could not open its window: {error}");
                cx.quit();
            }
        });
    Ok(())
}
