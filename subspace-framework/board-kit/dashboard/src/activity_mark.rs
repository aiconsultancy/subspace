//! Subspace's stationary mark with two moving highlights for indeterminate activity.

use std::f32::consts::PI;
use std::time::Duration;

use gpui_kit::{
    Animation, AnimationExt as _, App, Bounds, ContentMask, ElementId, Hsla, IntoElement, Path,
    PathBuilder, Pixels, Point, RenderOnce, Styled as _, Window, canvas, point, px, size,
};

use crate::Tokens;

/// Products that ship this artwork opt into the Subspace mark in their toolbar.
pub const TOOLBAR_ARTWORK: &str = "activity/subspace.svg";
const CYCLE: Duration = Duration::from_millis(2400);
const MAX_FPS: f32 = 24.;
const RADIUS: f32 = 104. / 240.;
const STROKE: f32 = 18. / 240.;
const HOOK_START: f32 = 3. * PI / 4.;
// The sweep is 180 degrees less the angle of the 48-unit return stroke.
const HOOK_SWEEP: f32 = 2.661_941;
const HOOK_END: f32 = 0.63;
const CONVERGE: f32 = 0.67;
const MEET: f32 = 0.79;
const FADE: f32 = 0.89;
const START: f32 = 0.;
const END: f32 = 1.;
const CENTER: f32 = 0.5;
const DIAGONAL_START: (f32, f32) = (0.0286, 0.9714);
const HOOK_TAIL: (f32, f32) = (0.2054, 0.5118);
const BASE_OPACITY: f32 = 0.78;
const HIGHLIGHT_BLEND: f32 = 0.72;
const FLASH_BLEND: f32 = 0.55;
const SPARK_FALLOFF: f32 = 7.;
const CONVERGENCE_OFFSET: f32 = 0.24;
const SPARK_LAYERS: [(f32, f32); 2] = [(0.36, 0.35), (0.23, END)];

/// Whether this product selected the animated Subspace logo for its toolbar.
pub fn in_product_toolbar(cx: &App) -> bool {
    cx.asset_source()
        .load(TOOLBAR_ARTWORK)
        .ok()
        .flatten()
        .is_some()
}

/// Decorative activity artwork. Pair it with text or an accessible status.
#[derive(IntoElement)]
pub struct ActivityMark {
    id: ElementId,
    size: Pixels,
    active: bool,
}

impl ActivityMark {
    pub fn new(id: impl Into<ElementId>, size: Pixels) -> Self {
        Self {
            id: id.into(),
            size,
            active: true,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

impl RenderOnce for ActivityMark {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let frame = MarkFrame {
            size: self.size,
            accent: tokens.accent,
            highlight: tokens.accent.blend(tokens.fg.opacity(HIGHLIGHT_BLEND)),
            phase: START,
        };
        if self.active {
            frame
                .with_animation(
                    self.id,
                    Animation::new(CYCLE).repeat_synced().with_max_fps(MAX_FPS),
                    |mut frame, phase| {
                        frame.phase = phase;
                        frame
                    },
                )
                .into_any_element()
        } else {
            frame.into_any_element()
        }
    }
}

#[derive(IntoElement)]
struct MarkFrame {
    size: Pixels,
    accent: Hsla,
    highlight: Hsla,
    phase: f32,
}

impl RenderOnce for MarkFrame {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let Some(mark) = outline(bounds) else {
                    return;
                };
                let flash = ((self.phase - CONVERGE) / (MEET - CONVERGE))
                    .min((FADE - self.phase) / (FADE - MEET))
                    .clamp(START, END);
                let base = self.accent.opacity(BASE_OPACITY);
                window.paint_path(
                    mark.clone(),
                    base.blend(self.highlight.opacity(flash * FLASH_BLEND)),
                );
                if let Some((at, strength)) = spark(self.phase)
                    && strength > START
                {
                    for center in [at, opposite(at)] {
                        shine(
                            window,
                            &mark,
                            bounds,
                            center,
                            self.highlight.opacity(strength),
                        );
                    }
                }
            },
        )
        .size(self.size)
        .flex_shrink_0()
    }
}

fn outline(bounds: Bounds<Pixels>) -> Option<Path> {
    let mut path = PathBuilder::stroke(bounds.size.width * STROKE);
    path.move_to(project(bounds, DIAGONAL_START));
    path.line_to(project(bounds, opposite(DIAGONAL_START)));
    let radius = bounds.size.width * RADIUS;
    let radii = point(radius, radius);
    let (start, end, tail) = (hook(START), hook(END), HOOK_TAIL);
    path.move_to(project(bounds, start));
    path.arc_to(radii, px(START), false, true, project(bounds, end));
    path.line_to(project(bounds, tail));
    path.move_to(project(bounds, opposite(start)));
    path.arc_to(
        radii,
        px(START),
        false,
        true,
        project(bounds, opposite(end)),
    );
    path.line_to(project(bounds, opposite(tail)));
    path.build().ok()
}

fn spark(phase: f32) -> Option<((f32, f32), f32)> {
    if phase < HOOK_END {
        let progress = phase / HOOK_END;
        let strength = (progress.min(END - progress) * SPARK_FALLOFF).min(END);
        return Some((hook(progress), strength));
    }
    if (CONVERGE..FADE).contains(&phase) {
        let progress = ((phase - CONVERGE) / (MEET - CONVERGE)).min(END);
        let offset = CONVERGENCE_OFFSET * (END - progress);
        let strength = if phase <= MEET {
            progress
        } else {
            (FADE - phase) / (FADE - MEET)
        };
        return Some(((CENTER - offset, CENTER + offset), strength));
    }
    None
}

fn hook(progress: f32) -> (f32, f32) {
    let (y, x) = (HOOK_START + progress * HOOK_SWEEP).sin_cos();
    (CENTER + x * RADIUS, CENTER + y * RADIUS)
}

fn opposite((x, y): (f32, f32)) -> (f32, f32) {
    (END - x, END - y)
}

fn project(bounds: Bounds<Pixels>, (x, y): (f32, f32)) -> Point<Pixels> {
    bounds.origin + point(bounds.size.width * x, bounds.size.height * y)
}

fn shine(window: &mut Window, mark: &Path, bounds: Bounds<Pixels>, at: (f32, f32), colour: Hsla) {
    let center = project(bounds, at);
    for (width, opacity) in SPARK_LAYERS {
        let extent = bounds.size.width * width;
        let bounds = Bounds::centered_at(center, size(extent, extent));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.paint_path(mark.clone(), colour.opacity(opacity));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparks_follow_mirrored_hooks_and_meet_on_the_diagonal() {
        let (start, strength) = spark(0.).unwrap();
        assert_eq!(strength, 0., "reduced motion uses the start of the cycle");
        assert!(start.0 < 0.5 && opposite(start).0 > 0.5);
        let (center, strength) = spark(MEET).unwrap();
        assert_eq!(center, opposite(center));
        assert_eq!(strength, 1.);
        assert!(spark(FADE).is_none());
    }
}
