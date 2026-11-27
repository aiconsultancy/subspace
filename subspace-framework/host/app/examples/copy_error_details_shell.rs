//! Actual About and Settings windows, seeded by the host's offline fixture API.

use std::path::PathBuf;

use gpui_kit::{WindowBounds, WindowOptions, px, size};
use subspace_product::{OsKind, PlatformEnv, ProductContext, fixture};
use subspace_shell::test_support::{Assets, copy_details_shell_fixture};

// Physical native window geometry is independent of the component spacing scale.
const WIDTH: f32 = 1200.;
const HEIGHT: f32 = 900.;

fn main() -> Result<(), String> {
    let (context, root, state) = arguments()?;
    gpui_kit::application()
        .with_assets(Assets::new(&[], &[]))
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.set_app_identity(
                "org.subspace.copy-details.shell-fixture",
                "Copy diagnostics fixture",
            );
            let options = WindowOptions {
                focus: false,
                inactive_frame_interval: None,
                // Native window dimensions are a platform boundary, not UI spacing.
                window_bounds: Some(WindowBounds::centered(
                    None,
                    size(px(WIDTH), px(HEIGHT)),
                    cx,
                )),
                ..Default::default()
            };
            if let Err(error) = gpui_kit::open_window(options, cx, |window, cx| {
                copy_details_shell_fixture(context, &root, &state, window, cx)
            }) {
                eprintln!("Cannot open the offline shell fixture: {error}");
                cx.quit();
            }
        });
    Ok(())
}

fn arguments() -> Result<(ProductContext, PathBuf, String), String> {
    let values = std::env::args().skip(1).collect::<Vec<_>>();
    let [product, directory, state] = values.as_slice() else {
        return Err(
            "Pass <product> <isolated-root> <about|update-about|update-compact|update-log>.".into(),
        );
    };
    let root = PathBuf::from(directory);
    if !matches!(
        state.as_str(),
        "about" | "update-about" | "update-compact" | "update-log"
    ) {
        return Err("Unknown shell fixture state.".into());
    }
    if !root.is_absolute()
        || std::env::var_os("HOME").as_deref() != Some(root.join("home").as_os_str())
    {
        return Err("Launch through the isolated evidence bundle with HOME=<root>/home.".into());
    }
    let known = fixture::products().map_err(|error| error.to_string())?;
    let descriptor = known
        .iter()
        .find(|entry| entry.id.as_str() == product.as_str())
        .cloned()
        .ok_or("Unknown product.")?;
    let env = PlatformEnv {
        os: OsKind::Macos,
        home: root.join("home"),
        xdg_config_home: Some(root.join("config")),
        xdg_state_home: Some(root.join("state")),
        app_data: None,
        local_app_data: None,
    };
    Ok((ProductContext::new(descriptor, env), root, state.to_owned()))
}
