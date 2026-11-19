//! Debug-only evidence capture of each product window to PNG. macOS reads
//! GPUI's rendered scene; Linux reads the native window on the capture
//! script's private X server. Neither path captures the user's desktop.
//!
//! `SUBSPACE_CAPTURE_DIR` enables it. `SUBSPACE_CAPTURE_STEPS` is a `;` list of
//! `wait:<ms>`, `action:<name>`, `click:<x>,<y>`, `right-click:<x>,<y>`,
//! `move:<x>,<y>`, `drag:<x1>,<y1>,<x2>,<y2>`, `scroll:<x>,<y>,<dy>`,
//! `type:<text>`, `key:<keystrokes>`, `drop:<x>,<y>,<fixture-path>`, `installed:<version>` and `shot:<name>`
//! steps, run in order
//! before the app quits. Points are window pixels; `move` puts the pointer
//! there without a click, for hover states; `drag` presses the left button
//! at the first point, moves to the second in steps and releases it there,
//! as a text selection does; `type` sends each character as a key press and `key` sends
//! space-separated keystrokes such as `down tab`; `installed` puts the updater
//! in the state after an update to `<version>` installed, without installing
//! anything. Actions dispatch at the app root independently of focus. Each
//! key step focuses the active Space, then sends its sequence to normal focus;
//! type steps preserve focus. The initial Space is the first unless fixture
//! state sets `app.dashboard`. The default waits four
//! seconds and takes one `main` shot.
#![cfg_attr(not(feature = "capture"), allow(dead_code))]

#[cfg(test)]
mod interaction_tests;
pub(crate) mod issues;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::capture_shot;
#[cfg(any(target_os = "linux", test))]
mod pixels;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyWindowHandle, App, AsyncApp, BorrowAppContext as _, Context, Div, ExternalPaths,
    FileDropEvent, FocusHandle, Global, InputEvent as _, InteractiveElement as _, Keystroke,
    Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput, Point,
    ScrollDelta, ScrollEvent, ScrollSource, Stateful, TouchPhase, WeakEntity, Window, point, px,
};

const DIR_ENV: &str = "SUBSPACE_CAPTURE_DIR";
const STEPS_ENV: &str = "SUBSPACE_CAPTURE_STEPS";
const DEFAULT_STEPS: &str = "wait:4000;shot:main";
const FRAME: Duration = Duration::from_millis(100);
/// How many pointer moves a drag takes from its start to its end.
const DRAG_MOVES: u16 = 10;
const DROP_FIELDS: usize = 3;

#[derive(Default)]
struct Shells(HashMap<AnyWindowHandle, Shell>);
impl Global for Shells {}

struct Shell {
    view: WeakEntity<crate::app::SubspaceApp>,
    focus: FocusHandle,
}

pub(crate) fn register(window: &Window, cx: &mut Context<crate::app::SubspaceApp>) {
    let shell = Shell {
        view: cx.weak_entity(),
        focus: cx.focus_handle(),
    };
    cx.update_default_global::<Shells, _>(|shells, _| {
        shells.0.insert(window.window_handle(), shell);
    });
}

pub(crate) fn root(element: Stateful<Div>, window: &Window, cx: &App) -> Stateful<Div> {
    let focus = target(window, cx).map(|shell| &shell.focus);
    element.when_some(focus, |element, focus| element.track_focus(focus))
}

fn target<'a>(window: &Window, cx: &'a App) -> Option<&'a Shell> {
    cx.try_global::<Shells>()
        .and_then(|shells| shells.0.get(&window.window_handle()))
}

#[derive(Debug, Clone, PartialEq)]
enum Step {
    Wait(Duration),
    Action(String),
    Click(f32, f32),
    RightClick(f32, f32),
    Move(f32, f32),
    Drag([f32; 4]),
    Drop(f32, f32, String),
    Scroll(f32, f32, f32),
    Type(String),
    Key(String),
    Installed(String),
    Account(String),
    Shot(String),
}

fn numbers(text: &str) -> Option<Vec<f32>> {
    text.split(',')
        .map(|part| part.trim().parse().ok())
        .collect()
}

fn parse_step(step: &str) -> Option<Step> {
    let (kind, value) = step.split_once(':')?;
    match (kind, numbers(value).as_deref()) {
        ("wait", _) => value
            .parse()
            .ok()
            .map(|ms| Step::Wait(Duration::from_millis(ms))),
        ("action", _) => Some(Step::Action(value.to_owned())),
        ("shot", _) => Some(Step::Shot(value.to_owned())),
        ("type", _) => Some(Step::Type(value.to_owned())),
        ("key", _) => Some(Step::Key(value.to_owned())),
        ("installed", _) => Some(Step::Installed(value.to_owned())),
        ("account", _) => Some(Step::Account(value.to_owned())),
        ("drop", _) => {
            let mut parts = value.splitn(DROP_FIELDS, ',');
            Some(Step::Drop(
                parts.next()?.trim().parse().ok()?,
                parts.next()?.trim().parse().ok()?,
                parts.next()?.to_owned(),
            ))
        }
        ("click", Some(&[x, y])) => Some(Step::Click(x, y)),
        ("right-click", Some(&[x, y])) => Some(Step::RightClick(x, y)),
        ("move", Some(&[x, y])) => Some(Step::Move(x, y)),
        ("drag", Some(&[x1, y1, x2, y2])) => Some(Step::Drag([x1, y1, x2, y2])),
        ("scroll", Some(&[x, y, dy])) => Some(Step::Scroll(x, y, dy)),
        _ => None,
    }
}

fn parse_steps(text: &str) -> Result<Vec<Step>, String> {
    text.split(';')
        .map(str::trim)
        .filter(|step| !step.is_empty())
        .map(|step| parse_step(step).ok_or_else(|| format!("bad capture step: {step}")))
        .collect()
}

pub(crate) fn start(cx: &mut App) {
    let Some(dir) = std::env::var_os(DIR_ENV).map(PathBuf::from) else {
        return;
    };
    issues::start(cx);
    let text = std::env::var(STEPS_ENV).unwrap_or_else(|_| DEFAULT_STEPS.to_owned());
    let steps = match parse_steps(&text) {
        Ok(steps) => steps,
        Err(error) => {
            failed(&error);
        }
    };
    cx.spawn(async move |cx| {
        for step in steps {
            if let Err(error) = run_step(&step, &dir, cx).await {
                failed(&error);
            }
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}

fn failed(error: &str) -> ! {
    eprintln!("capture failed: {error}");
    std::process::exit(1);
}

async fn run_step(step: &Step, dir: &Path, cx: &mut AsyncApp) -> Result<(), String> {
    match step {
        Step::Wait(duration) => {
            for tick in ticks(*duration) {
                cx.background_executor().timer(tick).await;
                cx.update(draw)?;
            }
            Ok(())
        }
        Step::Action(name) => cx.update(|cx| dispatch(name, cx)),
        Step::Click(x, y) => cx.update(|cx| click(point(px(*x), px(*y)), MouseButton::Left, cx)),
        Step::RightClick(x, y) => {
            cx.update(|cx| click(point(px(*x), px(*y)), MouseButton::Right, cx))
        }
        Step::Move(x, y) => cx.update(|cx| hover(point(px(*x), px(*y)), cx)),
        Step::Drag(points) => cx.update(|cx| send(drag_events(*points), cx)),
        Step::Drop(x, y, path) => {
            cx.update(|cx| send(drop_events(point(px(*x), px(*y)), PathBuf::from(path)), cx))
        }
        Step::Scroll(x, y, dy) => cx.update(|cx| scroll(point(px(*x), px(*y)), *dy, cx)),
        Step::Type(text) => cx.update(|cx| press(text.chars().map(String::from), cx)),
        Step::Key(keys) => cx.update(|cx| press_space(keys, cx)),
        Step::Installed(version) => cx.update(|cx| installed(version, cx)),
        Step::Account(state) => cx.update(|cx| crate::account::fixture(state, cx)),
        Step::Shot(name) => capture_shot(name, dir, cx).await,
    }
}

fn drop_events(position: Point<gpui_kit::Pixels>, path: PathBuf) -> Vec<PlatformInput> {
    vec![
        FileDropEvent::Entered {
            position,
            paths: ExternalPaths([path].into_iter().collect()),
        }
        .to_platform_input(),
        FileDropEvent::Submit { position }.to_platform_input(),
    ]
}

/// `duration` in frame-sized steps. macOS stops drawing a background window
/// it reports as occluded, and views start some loads while they render, so
/// a wait draws every window after each step.
fn ticks(duration: Duration) -> impl Iterator<Item = Duration> {
    let whole = duration.as_millis() / FRAME.as_millis();
    let rest = duration.saturating_sub(FRAME * whole as u32);
    std::iter::repeat_n(FRAME, whole as usize).chain((!rest.is_zero()).then_some(rest))
}

#[cfg(feature = "capture")]
fn installed(version: &str, cx: &mut App) -> Result<(), String> {
    crate::app::seed_installed_update(version, cx);
    Ok(())
}

#[cfg(not(feature = "capture"))]
fn installed(_: &str, _: &mut App) -> Result<(), String> {
    Err("installed needs the capture feature".into())
}

fn draw(cx: &mut App) -> Result<(), String> {
    for window in cx.windows() {
        window
            .update(cx, |_, window, cx| window.draw(cx).clear(cx))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn send(events: Vec<PlatformInput>, cx: &mut App) -> Result<(), String> {
    let window = cx.windows().into_iter().next().ok_or("no window")?;
    window
        .update(cx, |_, window, cx| {
            for event in events {
                window.dispatch_event(event, cx);
            }
        })
        .map_err(|e| e.to_string())
}

fn click(
    position: Point<gpui_kit::Pixels>,
    button: MouseButton,
    cx: &mut App,
) -> Result<(), String> {
    let down = MouseDownEvent {
        button,
        position,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    };
    let up = MouseUpEvent {
        button,
        position,
        modifiers: Modifiers::default(),
        click_count: 1,
    };
    send(
        vec![PlatformInput::MouseDown(down), PlatformInput::MouseUp(up)],
        cx,
    )
}

/// Puts the pointer at `position` without pressing a button.
fn hover(position: Point<gpui_kit::Pixels>, cx: &mut App) -> Result<(), String> {
    let moved = MouseMoveEvent {
        position,
        pressed_button: None,
        modifiers: Modifiers::default(),
    };
    send(vec![PlatformInput::MouseMove(moved)], cx)
}

/// A left-button drag from `(x1, y1)` to `(x2, y2)`: the press, the pointer
/// moving there in steps with the button held, and the release.
fn drag_events([x1, y1, x2, y2]: [f32; 4]) -> Vec<PlatformInput> {
    let at = |step: u16| {
        let t = f32::from(step) / f32::from(DRAG_MOVES);
        point(px(x1 + (x2 - x1) * t), px(y1 + (y2 - y1) * t))
    };
    let down = MouseDownEvent {
        button: MouseButton::Left,
        position: at(0),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    };
    let moves = (1..=DRAG_MOVES).map(|step| {
        PlatformInput::MouseMove(MouseMoveEvent {
            position: at(step),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        })
    });
    let up = MouseUpEvent {
        button: MouseButton::Left,
        position: at(DRAG_MOVES),
        modifiers: Modifiers::default(),
        click_count: 1,
    };
    std::iter::once(PlatformInput::MouseDown(down))
        .chain(moves)
        .chain([PlatformInput::MouseUp(up)])
        .collect()
}

fn scroll(position: Point<gpui_kit::Pixels>, dy: f32, cx: &mut App) -> Result<(), String> {
    let wheel = ScrollEvent {
        position,
        delta: ScrollDelta::Pixels(point(gpui_kit::Pixels::ZERO, px(dy))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    };
    send(vec![PlatformInput::Scroll(wheel)], cx)
}

/// Sends each keystroke to the first window as typed keys.
fn press(keystrokes: impl Iterator<Item = String>, cx: &mut App) -> Result<(), String> {
    let os = cx.platform_info().os;
    let keystrokes = keystrokes
        .map(|source| Keystroke::parse_with_os(&source, os).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let window = cx.windows().into_iter().next().ok_or("no window")?;
    window
        .update(cx, |_, window, cx| {
            for keystroke in keystrokes {
                window.dispatch_keystroke(keystroke, cx);
            }
        })
        .map_err(|e| e.to_string())
}

fn dispatch(name: &str, cx: &mut App) -> Result<(), String> {
    let action = cx
        .build_action(name, None)
        .map_err(|error| format!("action {name}: {error}"))?;
    let window = cx.windows().into_iter().next().ok_or("no window")?;
    window
        .update(cx, |_, window, cx| {
            window.draw(cx).clear(cx);
            let focus = target(window, cx)
                .ok_or("no capture window root")?
                .focus
                .clone();
            if !window.is_action_available_in(action.as_ref(), &focus) {
                return Err(format!("action {name}: no handler at the window root"));
            }
            // Synchronous dispatch on the shell's root bypasses focused inputs
            // without changing focus (or restoring focus over a handler's choice).
            focus.dispatch_action(action.as_ref(), window, cx);
            Ok(())
        })
        .map_err(|e| e.to_string())?
}

fn app(window: &Window, cx: &App) -> Result<gpui_kit::Entity<crate::app::SubspaceApp>, String> {
    target(window, cx)
        .and_then(|shell| shell.view.upgrade())
        .ok_or_else(|| "no capture app shell for this window".into())
}

fn press_space(keys: &str, cx: &mut App) -> Result<(), String> {
    let window = cx.windows().into_iter().next().ok_or("no window")?;
    window
        .update(cx, |_, window, cx| {
            let focus = app(window, cx)?
                .read(cx)
                .capture_space_focus(cx)
                .ok_or("no active Space")?;
            focus.focus(window, cx);
            window.draw(cx).clear(cx);
            Ok::<_, String>(())
        })
        .map_err(|e| e.to_string())??;
    press(keys.split_whitespace().map(String::from), cx)
}

#[cfg(not(target_os = "linux"))]
async fn capture_shot(name: &str, dir: &Path, cx: &mut AsyncApp) -> Result<(), String> {
    cx.update(|cx| draw(cx).and_then(|()| shoot(name, dir, cx)))
}

#[cfg(not(target_os = "linux"))]
fn shoot(name: &str, dir: &Path, cx: &mut App) -> Result<(), String> {
    for (index, window) in cx.windows().into_iter().enumerate() {
        let image = window
            .update(cx, |_, window, _| window.render_to_image())
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        save_image(name, dir, index, &image)?;
    }
    Ok(())
}

fn save_image(
    name: &str,
    dir: &Path,
    index: usize,
    image: &image::RgbaImage,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let file = match index {
        0 => dir.join(format!("{name}.png")),
        _ => dir.join(format!("{name}-{index}.png")),
    };
    image.save(&file).map_err(|error| error.to_string())?;
    eprintln!("captured {}", file.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui_kit::{MouseButton, PlatformInput, point, px};

    use super::{DRAG_MOVES, Step, drag_events, parse_steps, ticks};

    #[test]
    fn a_drag_presses_moves_in_steps_with_the_button_held_and_releases() {
        let events = drag_events([0., 10., 100., 30.]);
        assert_eq!(events.len(), usize::from(DRAG_MOVES) + 2);
        assert!(matches!(&events[0],
            PlatformInput::MouseDown(down) if down.position == point(px(0.), px(10.))));
        assert!(matches!(&events[5],
            PlatformInput::MouseMove(moved) if moved.position == point(px(50.), px(20.))
                && moved.pressed_button == Some(MouseButton::Left)));
        assert!(matches!(events.last(),
            Some(PlatformInput::MouseUp(up)) if up.position == point(px(100.), px(30.))));
    }

    #[test]
    fn waits_split_into_frames_that_add_up() {
        let steps: Vec<_> = ticks(Duration::from_millis(250)).collect();
        assert_eq!(
            steps,
            [100, 100, 50].map(Duration::from_millis).to_vec(),
            "two frames and the rest"
        );
        assert_eq!(ticks(Duration::from_millis(200)).count(), 2);
        assert_eq!(ticks(Duration::ZERO).count(), 0);
    }

    #[test]
    fn steps_parse_in_order_and_reject_unknown_kinds() {
        assert_eq!(
            parse_steps(
                "wait:10; action:app::Quit;click:4,5;right-click:6,7;move:8,9;drag:1,2,3,4;drop:5,6,/tmp/fixture.zip;scroll:1,2,-30;type:@gal;key:down tab;installed:1.2.0;shot:main;"
            )
            .unwrap(),
            vec![
                Step::Wait(Duration::from_millis(10)),
                Step::Action("app::Quit".into()),
                Step::Click(4., 5.),
                Step::RightClick(6., 7.),
                Step::Move(8., 9.),
                Step::Drag([1., 2., 3., 4.]),
                Step::Drop(5., 6., "/tmp/fixture.zip".into()),
                Step::Scroll(1., 2., -30.),
                Step::Type("@gal".into()),
                Step::Key("down tab".into()),
                Step::Installed("1.2.0".into()),
                Step::Shot("main".into()),
            ]
        );
        assert!(parse_steps("click:here").is_err());
        assert!(parse_steps("drag:1,2").is_err());
        assert!(parse_steps("wait:soon").is_err());
    }

    #[test]
    fn capture_failure_logs_the_action_and_exits_nonzero() {
        const CHILD_ENV: &str = "SUBSPACE_TEST_CAPTURE_FAILURE";
        if std::env::var_os(CHILD_ENV).is_some() {
            super::failed("action fixture::Missing: no handler");
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "capture::tests::capture_failure_logs_the_action_and_exits_nonzero",
                "--nocapture",
            ])
            .env(CHILD_ENV, "1")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("capture failed: action fixture::Missing: no handler")
        );
    }
}
