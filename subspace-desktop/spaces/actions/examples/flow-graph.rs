//! Offline screenshot fixture: cargo run -p subspace-actions-board --example flow-graph.
use gpui_kit::component::{Theme, ThemeSet};
use gpui_kit::{App, AppContext, WindowBounds, WindowOptions, px, size};
use subspace_actions_board::graph::{FlowGraph, GraphJob};

const WINDOW: (f32, f32) = (1440., 720.);
const ONE_DARK: &str = include_str!("themes/one-dark.json");
const SOLARIZED: &str = include_str!("themes/solarized.json");

fn fixtures() -> Vec<GraphJob> {
    [
        (
            1,
            "Build",
            "completed:success",
            "1m 42s",
            "4 / 4 steps completed",
        ),
        (
            2,
            "Test · Linux",
            "completed:success",
            "2m 18s",
            "5 / 5 steps completed",
        ),
        (
            3,
            "Test · macOS",
            "completed:success",
            "3m 09s",
            "5 / 5 steps completed",
        ),
        (
            4,
            "Test · Windows",
            "completed:success",
            "2m 51s",
            "5 / 5 steps completed",
        ),
        (
            5,
            "Package",
            "completed:success",
            "48s",
            "3 / 3 steps completed",
        ),
        (
            6,
            "Deploy staging",
            "in_progress",
            "1m 12s",
            "Health check · step 3 / 4",
        ),
        (
            7,
            "Deploy production",
            "waiting",
            "—",
            "Waiting for environment approval",
        ),
    ]
    .into_iter()
    .map(|(id, name, status, duration, steps)| GraphJob {
        id,
        name: name.into(),
        status: status.into(),
        duration: duration.into(),
        steps: steps.into(),
    })
    .collect()
}

fn main() {
    let mode = std::env::args().nth(1);
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            apply_theme(mode.as_deref(), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(
                    None,
                    size(px(WINDOW.0), px(WINDOW.1)),
                    cx,
                )),
                ..Default::default()
            };
            if let Err(error) = gpui_kit::open_window(options, cx, |_, cx| {
                cx.new(|cx| {
                    let mut graph = FlowGraph::new(cx);
                    graph.set_jobs(
                        fixtures(),
                        vec![
                            (1, 2),
                            (1, 3),
                            (1, 4),
                            (2, 5),
                            (3, 5),
                            (4, 5),
                            (5, 6),
                            (6, 7),
                        ],
                        None,
                        cx,
                    );
                    graph
                })
            }) {
                eprintln!("flow graph could not open its window: {error}");
                cx.quit();
            }
        });
}

fn apply_theme(mode: Option<&str>, cx: &mut App) {
    let (source, mode) = match mode {
        Some("light") => (SOLARIZED, "light"),
        Some("dark") => (SOLARIZED, "dark"),
        _ => (ONE_DARK, "dark"),
    };
    let Ok(set) = serde_json::from_str::<ThemeSet>(source) else {
        return;
    };
    let Some(theme) = set
        .themes
        .into_iter()
        .find(|theme| theme.mode.name() == mode)
    else {
        return;
    };
    let theme = std::rc::Rc::new(theme);
    Theme::update(cx, |current| current.apply_config(&theme));
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;
    use gpui_kit::component::Theme;

    use super::{apply_theme, fixtures};

    #[test]
    fn the_fixture_is_seven_jobs_with_their_steps() {
        let jobs = fixtures();
        assert_eq!(jobs.len(), 7);
        assert_eq!(
            jobs.iter().map(|job| job.id).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6, 7]
        );
        assert_eq!(jobs[0].name, "Build");
        assert!(
            jobs.iter()
                .all(|job| !job.status.is_empty() && !job.duration.is_empty())
        );
    }

    #[gpui_kit::test]
    fn the_mode_argument_picks_the_theme(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            apply_theme(Some("light"), cx);
            assert!(!Theme::global(cx).mode.is_dark());
            apply_theme(None, cx);
            assert!(Theme::global(cx).mode.is_dark());
        });
    }
}
