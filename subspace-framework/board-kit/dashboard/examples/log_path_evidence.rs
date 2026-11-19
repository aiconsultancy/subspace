//! Production LogView with synthetic records and an isolated configured writer.
//! Run `cargo run -p subspace-dashboard --example log_path_evidence -- wide`
//! or use `narrow` for the responsive toolbar. `no-writer` skips file logging.
//! With `--features test-support`, `seeded-overflow` opens the real menu after layout.
//! The same source works on origin/main: it does not call the new path API.

use gpui_kit::{AppContext as _, WindowBounds, WindowOptions, px, size};
use subspace_core::log::{self, Config, Level, Logger};
use subspace_dashboard::log_view::LogView;

// Native window dimensions are physical geometry, not component spacing.
const WIDE: f32 = 1440.;
const NARROW: f32 = 380.;
const HEIGHT: f32 = 420.;
#[cfg(feature = "test-support")]
const SEED_FRAME_LIMIT: usize = 4;

fn validate_seed_support(requested: bool) -> Result<(), String> {
    if requested && !cfg!(feature = "test-support") {
        Err("seeded-overflow requires --features test-support".into())
    } else {
        Ok(())
    }
}

#[cfg(feature = "test-support")]
fn prime_overflow_seed(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
) -> Result<(), &'static str> {
    let position = measured_more_position(window, cx)?;
    dispatch_seed_click(position, window, cx);
    verify_overflow_seed(window, cx)
}

#[cfg(feature = "test-support")]
fn measured_more_position(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
) -> Result<gpui_kit::Point<gpui_kit::Pixels>, &'static str> {
    use gpui_kit::test::TestWindowExt as _;
    for _ in 0..SEED_FRAME_LIMIT {
        window.render_frame(cx);
        if let Some(trigger) = window
            .try_find("overflow-toolbar-button")
            .filter(|trigger| trigger.visible())
        {
            return Ok(trigger.bounds().center());
        }
    }
    Err("no measured More toolbar actions button")
}

#[cfg(feature = "test-support")]
fn dispatch_seed_click(
    position: gpui_kit::Point<gpui_kit::Pixels>,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
) {
    use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, MouseUpEvent};
    // Match GPUI's AX Click fallback inside this window only. No OS input.
    window.dispatch_event(
        MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Default::default(),
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
    window.dispatch_event(
        MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Default::default(),
            click_count: 1,
        }
        .to_platform_input(),
        cx,
    );
    window.refresh();
}

#[cfg(feature = "test-support")]
fn verify_overflow_seed(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
) -> Result<(), &'static str> {
    use gpui_kit::test::TestWindowExt as _;
    for _ in 0..SEED_FRAME_LIMIT {
        window.render_frame(cx);
        if window
            .try_find("popup-menu")
            .is_some_and(|menu| menu.visible())
        {
            return Ok(());
        }
    }
    Err("production menu did not open")
}

fn seed(directory: &std::path::Path, writer: bool) -> Result<(), String> {
    if writer {
        log::init(Config::new(
            directory.to_owned(),
            "fixture-product",
            Level::Info,
        ))?;
    }
    let logger = Logger::new("fixture");
    logger.info("Synthetic app started");
    logger.info("Synthetic connection ready");
    logger.warn("Synthetic retry scheduled");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let seeded = arguments.iter().any(|arg| arg == "seeded-overflow");
    validate_seed_support(seeded)?;
    let width = if arguments.iter().any(|arg| arg == "narrow") {
        NARROW
    } else {
        WIDE
    };
    let fixture = tempfile::Builder::new()
        .prefix("subspace-log-path-fixture-")
        .tempdir()?;
    seed(
        fixture.path(),
        !arguments.iter().any(|arg| arg == "no-writer"),
    )?;
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            subspace_dashboard::bind_keys(cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(width), px(HEIGHT)), cx)),
                focus: false,
                show: true,
                ..Default::default()
            };
            let opened = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| LogView::new(window, cx))
            });
            match opened {
                #[cfg(feature = "test-support")]
                Ok((handle, _)) if seeded => {
                    // Root is installed and initially drawn. Prime the real scene
                    // before yielding setup; native presentation remains unchanged.
                    match handle.update(cx, |_, window, cx| prime_overflow_seed(window, cx)) {
                        Ok(Ok(())) => eprintln!("seeded-overflow: production menu scene is ready"),
                        failure => {
                            eprintln!("seeded-overflow failed: {failure:?}");
                            cx.quit();
                        }
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    eprintln!("log_path_evidence could not open its window: {error}");
                    cx.quit();
                }
            }
        });
    // Keep the isolated writer directory alive for the entire native app run.
    drop(fixture);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_seed_support;

    #[test]
    fn existing_arguments_do_not_require_test_support() {
        assert!(validate_seed_support(false).is_ok());
    }

    #[test]
    fn seeded_argument_requires_explicit_test_support() {
        assert_eq!(
            validate_seed_support(true).is_ok(),
            cfg!(feature = "test-support")
        );
    }

    #[cfg(feature = "test-support")]
    #[gpui_kit::test]
    fn synchronous_seed_opens_the_real_production_overflow(cx: &mut gpui_kit::TestAppContext) {
        use super::{HEIGHT, NARROW, prime_overflow_seed};
        use gpui_kit::test::TestWindowExt as _;
        use gpui_kit::{AppContext as _, VisualTestContext, WindowBounds, WindowOptions, px, size};
        use subspace_dashboard::log_view::LogView;

        cx.update(gpui_kit::init);
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(gpui_kit::Bounds::new(
                        Default::default(),
                        size(px(NARROW), px(HEIGHT)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| LogView::new(window, cx)),
            )
            .unwrap()
        });
        let cx = &mut VisualTestContext::from_window(handle, cx);
        cx.update(|window, cx| {
            assert_eq!(prime_overflow_seed(window, cx), Ok(()));
            assert!(window.find("popup-menu").visible());
            let menu = window.within("popup-menu");
            for label in ["Copy", "Open log folder"] {
                assert!((0usize..32).any(|ix| {
                    menu.try_find(ix)
                        .is_some_and(|item| item.visible() && item.label() == Some(label))
                }));
            }
        });
    }
}
