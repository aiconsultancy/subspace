//! Native evidence harness. Parent launches its isolated bundle in background.
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Window,
    WindowBounds, WindowOptions, div, px, size,
};
use subspace_client_github::issue_report::{Draft, ErrorKind, ReportPlan};
use subspace_core::diagnostics::{LaunchKind, LocalDiagnostics, RedactionContext, RuntimeDetails};
use subspace_report_issue::{
    Attachment, Destination, Diagnostics, IssuePaths, ReportIssue, prepare,
};

const FIXTURE_WIDTH: u32 = 120;
const FIXTURE_HEIGHT: u32 = 60;
const FIXTURE_IMAGE_COUNT: usize = 5;
const LAST_IMAGE_PREDECESSOR: usize = FIXTURE_IMAGE_COUNT - 1;
const CHART_COLUMN_WIDTH: u32 = 20;
const CHART_TOP_INSET: u32 = 8;
const CHART_BOTTOM_INSET: u32 = 5;
const CHART_COLUMN_RISE: u32 = 7;
const CHART_IMAGE_RISE: u32 = 5;
const CHART_HEIGHT_CYCLE: u32 = 40;
const CHART_BAR_WIDTH: u32 = 14;
const CHART_BACKGROUND_RGB: [u8; 3] = [236, 240, 244];
// Raster data only, not UI theme colours. Each test image has a distinct chart.
const FIXTURE_RGB: [[u8; 3]; FIXTURE_IMAGE_COUNT] = [
    [30, 100, 160],
    [32, 132, 92],
    [174, 96, 38],
    [128, 72, 152],
    [170, 64, 86],
];
// Native window geometry keeps the production dialog visible without activation.
const WINDOW_WIDTH: f32 = 1100.;
const WINDOW_HEIGHT: f32 = 1100.;
const LONG_REPORT_REPEATS: usize = 400;

mod offline;

use offline::offline_destination;

struct Fixture {
    _report: Entity<ReportIssue>,
    _directory: tempfile::TempDir,
    seed: Option<Seed>,
}

struct Seed {
    mode: String,
    draft: Draft,
    images: Vec<Attachment>,
    plan: ReportPlan,
    paths: Option<IssuePaths>,
}

impl Seed {
    fn new(
        mode: String,
        destination: &Destination,
        diagnostics: &Diagnostics,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let draft = synthetic_draft(&mode);
        let images = synthetic_images(&mode)?;
        let paths = matches!(mode.as_str(), "paths-off" | "paths-on" | "paths-preview")
            .then(synthetic_paths);
        let effective = match paths.as_ref().filter(|_| includes_paths(&mode)) {
            Some(paths) => diagnostics.clone().with_paths(paths.clone()),
            None => diagnostics.clone().without_paths(),
        };
        let report = prepare(&draft, &effective, &destination.repository)?;
        let plan = destination.client.prepare_issue_report(report);
        Ok(Self {
            mode,
            draft,
            images,
            plan,
            paths,
        })
    }

    fn apply(self, report: &mut ReportIssue, window: &mut Window, cx: &mut Context<ReportIssue>) {
        report.fixture_draft(&self.draft, window, cx);
        for image in self.images {
            report.fixture_attachment(image, cx);
        }
        report.fixture_diagnostics_expanded(self.mode.ends_with("-expanded"), cx);
        if let Some(paths) = self.paths {
            report.fixture_paths(paths, includes_paths(&self.mode), cx);
        }
        match self.mode.as_str() {
            "browser" | "browser-1" | "browser-5" => {
                report.fixture_browser(self.plan, window, cx);
                eprintln!(
                    "Seeded Browser result with exact reviewed title/body; no browser or clipboard actions."
                );
            }
            "preview" | "preview-5" | "preview-5-end" | "paths-preview" => {
                report.fixture_preview(self.plan, window, cx)
            }
            "created-1" | "created-5" => report.fixture_created(cx),
            "copied-5" | "copied-5-last" => {
                report.fixture_created(cx);
                let copied = if self.mode.ends_with("-last") {
                    LAST_IMAGE_PREDECESSOR
                } else {
                    1
                };
                report.fixture_copy_progress(copied, cx);
            }
            "saved-5" => {
                report.fixture_created(cx);
                report.fixture_save_images(cx);
            }
            "error" => report.fixture_error(ErrorKind::AccessDenied.into(), cx),
            "validation" => {
                report.fixture_draft(&Draft::default(), window, cx);
                report.fixture_error(ErrorKind::DescriptionRequired.into(), cx);
            }
            _ => {}
        }
    }
}
impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child("Synthetic report fixture")
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Cannot prepare report fixture: {error}");
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "form".into());
    let appearance = Appearance::from_argument(std::env::args().nth(2).as_deref());
    let directory = tempfile::tempdir()?;
    let destination = offline_destination(directory.path())?;
    let diagnostics = synthetic_diagnostics(appearance);
    let seed = Seed::new(mode, &destination, &diagnostics)?;
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            Theme::change(appearance.mode, None, cx);
            let opened = gpui_kit::open_window(window_options(cx), cx, |window, cx| {
                let directory_path = directory.path().into();
                let report = cx.new(|cx| {
                    ReportIssue::new(destination, diagnostics, directory_path, window, cx)
                });
                cx.new(|_| Fixture {
                    _report: report,
                    _directory: directory,
                    seed: Some(seed),
                })
            });
            let (handle, fixture) = match opened {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("Cannot open fixture window: {error}");
                    cx.quit();
                    return;
                }
            };
            if let Err(error) = cx.update_window(handle, |_, window, cx| {
                show_fixture(&fixture, window, cx);
            }) {
                eprintln!("Cannot show report fixture: {error}");
                cx.quit();
            }
        });
    Ok(())
}

fn show_fixture(fixture: &Entity<Fixture>, window: &mut Window, cx: &mut App) {
    let (report, seed) = fixture.update(cx, |fixture, _| {
        (fixture._report.clone(), fixture.seed.take())
    });
    ReportIssue::show(&report, window, cx);
    if let Some(seed) = seed {
        let reveal_paths = matches!(seed.mode.as_str(), "paths-preview" | "preview-5-end");
        report.update(cx, |report, cx| seed.apply(report, window, cx));
        if reveal_paths {
            ReportIssue::fixture_preview_end(&report, window, cx);
        }
    }
}

fn includes_paths(mode: &str) -> bool {
    matches!(mode, "paths-on" | "paths-preview")
}

/// The fixture's theme: the window's mode and the theme its diagnostics name.
#[derive(Clone, Copy)]
struct Appearance {
    mode: ThemeMode,
    theme: &'static str,
}

impl Appearance {
    const LIGHT: Self = Self {
        mode: ThemeMode::Light,
        theme: "default-light",
    };
    const DARK: Self = Self {
        mode: ThemeMode::Dark,
        theme: "default-dark",
    };

    /// `light` selects the light theme; anything else, or nothing, the dark one.
    fn from_argument(argument: Option<&str>) -> Self {
        if argument == Some("light") {
            Self::LIGHT
        } else {
            Self::DARK
        }
    }
}

fn synthetic_diagnostics(appearance: Appearance) -> Diagnostics {
    Diagnostics::new("subspace", "1.15.0")
        .with_build("dev", "fixture")
        .with_platform("macos", "aarch64")
        .with_appearance(appearance.theme, "medium")
        .with_active_board("subspace")
}

fn synthetic_paths() -> IssuePaths {
    LocalDiagnostics::default()
        .with_runtime(RuntimeDetails::new(
            "/Users/fixture/Applications/Subspace.app/Contents/MacOS/subspace",
            LaunchKind::Desktop,
            "login shell /Users/fixture/bin/zsh",
            Some("/Users/fixture/bin/az".into()),
        ))
        .with_redaction(RedactionContext::new(Some("/Users/fixture".into()), []))
        .issue_paths(Some("/Users/fixture/config/fixture-tunnel.json"))
}

fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        focus: false,
        inactive_frame_interval: None,
        window_bounds: Some(WindowBounds::centered(
            size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)),
            cx,
        )),
        ..Default::default()
    }
}

fn synthetic_draft(mode: &str) -> Draft {
    if mode.starts_with("browser") {
        Draft::new(format!(
            "Synthetic browser recovery report\n{}",
            "Fixture-only details. ".repeat(LONG_REPORT_REPEATS)
        ))
        .with_title("Synthetic browser title")
    } else {
        Draft::new("The synthetic preview does not resize\nFixture data only.")
            .with_title("Fixture panel does not resize")
            .with_steps("1. Open a fixture\n2. Resize its panel")
    }
}

fn image_count(mode: &str) -> usize {
    match mode.split('-').nth(1) {
        Some("0") => 0,
        Some("1") => 1,
        Some("5") => FIXTURE_IMAGE_COUNT,
        _ => usize::from(matches!(mode, "image" | "preview" | "browser")),
    }
}

fn synthetic_images(mode: &str) -> Result<Vec<Attachment>, Box<dyn std::error::Error>> {
    (0..image_count(mode)).map(synthetic_image).collect()
}

fn synthetic_image(ix: usize) -> Result<Attachment, Box<dyn std::error::Error>> {
    let accent = FIXTURE_RGB[ix];
    let pixels = image::RgbImage::from_fn(FIXTURE_WIDTH, FIXTURE_HEIGHT, |x, y| {
        let column = x / CHART_COLUMN_WIDTH;
        let rise = (column * CHART_COLUMN_RISE + ix as u32 * CHART_IMAGE_RISE) % CHART_HEIGHT_CYCLE;
        let bar_top = FIXTURE_HEIGHT - CHART_TOP_INSET - rise;
        if y >= bar_top
            && y < FIXTURE_HEIGHT - CHART_BOTTOM_INSET
            && x % CHART_COLUMN_WIDTH < CHART_BAR_WIDTH
        {
            image::Rgb(accent)
        } else {
            image::Rgb(CHART_BACKGROUND_RGB)
        }
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(pixels).write_to(&mut bytes, image::ImageFormat::Png)?;
    Ok(Attachment::from_bytes(bytes.into_inner())?)
}

#[cfg(test)]
mod tests {
    use super::{
        Appearance, Seed, image_count, offline_destination, synthetic_diagnostics, synthetic_draft,
        synthetic_images,
    };
    use subspace_client_github::issue_report::{BrowserMode, Diagnostics, prepare};

    #[test]
    fn path_presets_prepare_only_explicit_synthetic_path_variants() {
        let directory = tempfile::tempdir().unwrap();
        let destination = offline_destination(directory.path()).unwrap();
        for (mode, included) in [
            ("paths-off", false),
            ("paths-on", true),
            ("paths-preview", true),
        ] {
            let seed = Seed::new(
                mode.into(),
                &destination,
                &Diagnostics::new("subspace", "1.15.0"),
            )
            .unwrap();
            assert!(seed.paths.is_some());
            assert_eq!(
                seed.plan
                    .body()
                    .contains("- Executable: ~/Applications/Subspace.app/Contents/MacOS/subspace"),
                included
            );
            assert_eq!(
                seed.plan
                    .body()
                    .contains("- Configuration file: ~/config/fixture-tunnel.json"),
                included
            );
            assert!(!seed.plan.body().contains("/Users/fixture"));
        }
    }

    #[test]
    fn browser_preset_requires_manual_body_paste() {
        let report = prepare(
            &synthetic_draft("browser"),
            &Diagnostics::new("subspace", "1.15.0"),
            &subspace_report_issue::IssueRepository::parse("example-org/example-app").unwrap(),
        )
        .unwrap();
        assert_eq!(
            report.browser_fallback().editor_mode(),
            BrowserMode::ClipboardRequired
        );
        assert_eq!(report.title(), "Synthetic browser title");
    }

    #[test]
    fn form_matrix_uses_exact_image_counts_and_real_report_fields() {
        let directory = tempfile::tempdir().unwrap();
        let destination = offline_destination(directory.path()).unwrap();
        for count in [0, 1, 5] {
            for disclosure in ["collapsed", "expanded"] {
                let mode = format!("form-{count}-{disclosure}");
                let seed = Seed::new(
                    mode.clone(),
                    &destination,
                    &Diagnostics::new("subspace", "1.15.0"),
                )
                .unwrap();
                assert_eq!(image_count(&mode), count);
                assert_eq!(seed.images.len(), count);
                assert_eq!(seed.plan.title(), "Fixture panel does not resize");
                assert!(
                    seed.plan
                        .body()
                        .contains("The synthetic preview does not resize")
                );
                assert!(seed.plan.body().contains("1. Open a fixture"));
                assert!(seed.plan.body().contains("2. Resize its panel"));
            }
        }
    }

    #[test]
    fn fixture_images_are_distinct_validated_pngs() {
        let images = synthetic_images("form-5-expanded").unwrap();
        assert_eq!(images.len(), 5);
        let directory = tempfile::tempdir().unwrap();
        let paths: Vec<_> = (0..images.len())
            .map(|ix| directory.path().join(format!("fixture-{ix}.png")))
            .collect();
        for (image, path) in images.iter().zip(&paths) {
            image.save_new(path).unwrap();
        }
        let bytes: Vec<_> = paths
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect();
        for pair in bytes.windows(2) {
            assert_ne!(pair[0], pair[1]);
        }
    }

    #[test]
    fn preview_fixture_contains_all_nine_permitted_diagnostic_fields() {
        for (argument, theme) in [(None, "default-dark"), (Some("light"), "default-light")] {
            let appearance = Appearance::from_argument(argument);
            let report = prepare(
                &synthetic_draft("preview-5"),
                &synthetic_diagnostics(appearance),
                &subspace_report_issue::IssueRepository::parse("example-org/example-app").unwrap(),
            )
            .unwrap();
            let lines: Vec<_> = report
                .body()
                .lines()
                .filter(|line| line.starts_with("- "))
                .collect();
            assert_eq!(lines.len(), 9);
            for name in [
                "Product",
                "App version",
                "Build channel",
                "Commit",
                "OS",
                "Architecture",
                "Active board",
                "Theme",
                "Icon size",
            ] {
                assert!(
                    lines
                        .iter()
                        .any(|line| line.starts_with(&format!("- {name}:")))
                );
            }
            assert!(report.body().contains(theme));
        }
    }
}
