use std::path::Path;

use gpui_kit::base::Root;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, HeadlessAppContext, IntoElement,
    ParentElement as _, Render, Styled as _, Window, px, size,
};
use serde_json::{Value, json};
use subspace_component_gallery_board::ComponentGallery;
use subspace_dashboard::{Dashboard as _, Tokens};

use super::settings::{CaptureCase, SCALE, WIDTH, bounded_height};

struct Evidence {
    gallery: Entity<ComponentGallery>,
    item: String,
}

impl Render for Evidence {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let card = self.gallery.update(cx, |gallery, cx| {
            gallery
                .render_test_item(&self.item, window, cx)
                .expect("fixture must use an existing production catalogue card")
        });
        let tokens = Tokens::current(cx);
        v_flex()
            .size_full()
            .p_4()
            .bg(cx.theme().background)
            .font_family(cx.theme().font_family.clone())
            .text_size(tokens.row_font)
            .text_color(tokens.fg)
            // Keep natural height when measuring a card taller than the viewport.
            // This wrapper changes no card styles or sample dimensions.
            .child(v_flex().flex_shrink_0().child(card))
    }
}

fn open_fixture(cx: &mut HeadlessAppContext, item: &str, height: u32) -> AnyWindowHandle {
    let window = cx
        .open_window(size(px(WIDTH as f32), px(height as f32)), |window, cx| {
            let gallery = cx.new(|cx| ComponentGallery::new(window, cx));
            let view = cx.new(|_| Evidence {
                gallery,
                item: item.into(),
            });
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("TestPlatform must create an offscreen window with a real headless renderer");
    window.into()
}

fn close_fixture(cx: &mut HeadlessAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, _| window.remove_window())
        .expect("close fixture window before the next card");
    cx.run_until_parked();
}

pub(crate) fn capture_case(
    cx: &mut HeadlessAppContext,
    case: &CaptureCase,
    path: &Path,
    minimum: u32,
) -> Value {
    let mut window = open_fixture(cx, &case.item, minimum);
    let height = measure_height(cx, window, &case.item).max(minimum);
    if height > minimum {
        close_fixture(cx, window);
        window = open_fixture(cx, &case.item, height);
    }
    let record = capture_card(cx, window, case, path, height);
    close_fixture(cx, window);
    record
}

fn measure_height(cx: &mut HeadlessAppContext, window: AnyWindowHandle, item: &str) -> u32 {
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bounds = window.find(format!("gallery-item:{item}")).bounds();
        // Preserve the same bottom inset as the existing top inset.
        bounded_height(f32::from(bounds.bottom() + bounds.origin.y))
    })
    .expect("measure production card at its natural height")
}

fn capture_card(
    cx: &mut HeadlessAppContext,
    window: AnyWindowHandle,
    case: &CaptureCase,
    path: &Path,
    height: u32,
) -> Value {
    cx.run_until_parked();
    let (record, image) = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.scale_factor(),
                SCALE as f32,
                "pinned test display scale"
            );
            let card = window.find(format!("gallery-item:{}", case.item));
            let bounds = card.bounds();
            assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
            assert!(bounds.origin.x >= px(0.) && bounds.origin.y >= px(0.));
            assert!(
                bounds.right() <= px(WIDTH as f32),
                "card clipped horizontally"
            );
            assert!(
                bounds.bottom() <= px(height as f32),
                "card clipped vertically"
            );
            let record = json!({
                "item": case.item,
                "slug": case.slug,
                "theme": cx.theme().theme_name().as_ref(),
                "png": path.file_name().unwrap().to_str().unwrap(),
                "width": WIDTH,
                "height": height,
                "scaleFactor": SCALE,
                "cardBounds": {
                    "x": f32::from(bounds.origin.x), "y": f32::from(bounds.origin.y),
                    "width": f32::from(bounds.size.width),
                    "height": f32::from(bounds.size.height),
                },
            });
            let image = window.render_to_image().expect("GPU scene readback");
            (record, image)
        })
        .expect("render actual catalogue card");
    assert_eq!(image.dimensions(), (WIDTH * SCALE, height * SCALE));
    let first = image.get_pixel(0, 0);
    assert!(
        image.pixels().any(|pixel| pixel != first),
        "blank GPU image"
    );
    image.save(path).expect("encode PNG");
    let decoded = image::open(path).expect("decode saved PNG").into_rgba8();
    assert_eq!(decoded, image, "saved PNG must preserve GPU readback");
    record
}
