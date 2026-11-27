//! One actual gallery catalogue card, without navigation or replacement markup.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, Window,
    WindowBounds, WindowOptions, px, size,
};
use subspace_component_gallery_board::ComponentGallery;
use subspace_dashboard::{Dashboard as _, prefs};

// Physical native window geometry is independent of the component spacing scale.
const WIDTH: f32 = 1000.;
const HEIGHT: f32 = 600.;

struct Evidence {
    gallery: Entity<ComponentGallery>,
    item: &'static str,
}

impl Render for Evidence {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let card = self.gallery.update(cx, |gallery, cx| {
            gallery.render_test_item(self.item, window, cx)
        });
        v_flex()
            .size_full()
            .p_4()
            .bg(cx.theme().background)
            .children(card)
    }
}

fn main() -> Result<(), String> {
    let item = arguments()?;
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
            cx.set_app_identity(
                "org.subspace.copy-details.gallery-fixture",
                "Gallery fixture",
            );
            let options = WindowOptions {
                focus: false,
                inactive_frame_interval: None,
                window_bounds: Some(WindowBounds::centered(
                    None,
                    size(px(WIDTH), px(HEIGHT)),
                    cx,
                )),
                ..Default::default()
            };
            if let Err(error) = gpui_kit::open_window(options, cx, move |window, cx| {
                cx.new(|cx| Evidence {
                    gallery: cx.new(|cx| ComponentGallery::new(window, cx)),
                    item,
                })
            }) {
                eprintln!("Cannot open the offline gallery fixture: {error}");
                cx.quit();
            }
        });
    Ok(())
}

fn arguments() -> Result<&'static str, String> {
    let values = std::env::args().skip(1).collect::<Vec<_>>();
    let [product, root, state] = values.as_slice() else {
        return Err("Pass gallery <isolated-root> <copy-details|error-box>.".into());
    };
    let home = std::path::Path::new(root).join("home");
    if product != "gallery"
        || !home.is_absolute()
        || std::env::var_os("HOME").as_deref() != Some(home.as_os_str())
    {
        return Err("Launch through the isolated gallery evidence bundle.".into());
    }
    match state.as_str() {
        "copy-details" => Ok("copy_details::CopyDetails"),
        "error-box" => Ok("feedback::error_box"),
        _ => Err("Unknown catalogue fixture state.".into()),
    }
}
