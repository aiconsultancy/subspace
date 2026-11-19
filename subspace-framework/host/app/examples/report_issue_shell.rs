//! Actual shell with an inert board for background-only before/after captures.

use std::path::PathBuf;

use gpui_kit::{WindowBounds, WindowOptions, px, size};
use subspace_product::{OsKind, PlatformEnv, ProductContext, fixture};
use subspace_shell::test_support::{Assets, offline_shell_fixture};

const FIXTURE_WIDTH: f32 = 1440.;
const FIXTURE_HEIGHT: f32 = 850.;

fn main() -> Result<(), String> {
    let (context, root) = fixture_arguments()?;
    gpui_kit::application()
        .with_assets(Assets::new(&[], &[]))
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.set_app_identity("org.subspace.report-issue.fixture", "Report issue fixture");
            let options = WindowOptions {
                focus: false,
                inactive_frame_interval: None,
                window_bounds: Some(WindowBounds::centered(
                    size(px(FIXTURE_WIDTH), px(FIXTURE_HEIGHT)),
                    cx,
                )),
                ..Default::default()
            };
            if let Err(error) = gpui_kit::open_window(options, cx, |window, cx| {
                offline_shell_fixture(context, &root, window, cx)
            }) {
                eprintln!("Cannot open the offline fixture window: {error}");
                cx.quit();
            }
        });
    Ok(())
}

fn fixture_arguments() -> Result<(ProductContext, PathBuf), String> {
    let mut args = std::env::args().skip(1);
    let id = args.next().ok_or("Pass a product descriptor ID.")?;
    let path = args
        .next()
        .ok_or("Pass an absolute isolated fixture directory.")?;
    let root = absolute_root(PathBuf::from(path))?;
    let known = fixture::products().map_err(|error| error.to_string())?;
    let product = known
        .iter()
        .find(|product| product.id.as_str() == id)
        .ok_or("Unknown fixture product.")?
        .clone();
    let env = PlatformEnv {
        os: OsKind::Macos,
        home: root.join("home"),
        xdg_config_home: Some(root.join("config")),
        xdg_state_home: Some(root.join("state")),
        app_data: None,
        local_app_data: None,
    };
    Ok((ProductContext::new(product, env), root))
}

fn absolute_root(root: PathBuf) -> Result<PathBuf, String> {
    root.is_absolute()
        .then_some(root)
        .ok_or("Fixture directory must be absolute.".into())
}
