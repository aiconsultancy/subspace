//! Reporting the pointer to a program that asked for it (REQ-23).
//!
//! The encoder and the event object are created once and reused, as
//! libghostty intends. [`MouseWriter::encode`] reads the program's current
//! tracking mode and output format with `set_options_from_terminal` and the
//! session's geometry with `set_size`, so a pixel position becomes the cell
//! the program expects without the view converting anything itself.

use gpui_kit::{
    Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
};
use libghostty_vt::Terminal;
use libghostty_vt::mouse::{Action, Button, Encoder, EncoderSize, Event, Position};

use crate::error::{SessionError, emulator_error};
use crate::input::keys::mods_of;
use crate::metrics::Viewport;

/// How much room one encoded pointer event usually needs.
const INITIAL_BYTES: usize = 32;

/// The smallest cell libghostty accepts, which it requires to be non-zero.
const MIN_CELL: u32 = 1;

/// One pointer event, in the session's own terms.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PointerEvent {
    /// Press, release or motion.
    pub(crate) action: Action,
    /// The button, and nothing for a motion with no button held.
    pub(crate) button: Option<Button>,
    /// The modifiers held down.
    pub(crate) mods: libghostty_vt::key::Mods,
    /// Where the pointer is, in the session's own pixels.
    pub(crate) position: Position,
}

/// One session's mouse encoder, its event object and the bytes it produced
/// last.
pub(crate) struct MouseWriter {
    encoder: Encoder<'static>,
    event: Event<'static>,
    bytes: Vec<u8>,
}

impl MouseWriter {
    /// The encoder for one session.
    pub(crate) fn new() -> Result<Self, SessionError> {
        Ok(Self {
            encoder: Encoder::new().map_err(emulator_error)?,
            event: Event::new().map_err(emulator_error)?,
            bytes: Vec::with_capacity(INITIAL_BYTES),
        })
    }

    /// The bytes one pointer event sends the program in its current mode,
    /// which is empty for an event the program does not receive.
    pub(crate) fn encode(
        &mut self,
        terminal: &Terminal<'static, 'static>,
        viewport: Viewport,
        event: &PointerEvent,
        pressed: bool,
    ) -> Result<&[u8], SessionError> {
        self.encoder.set_options_from_terminal(terminal);
        self.encoder.set_size(encoder_size(viewport));
        self.encoder.set_any_button_pressed(pressed);
        self.encoder.set_track_last_cell(true);
        self.event.set_action(event.action);
        self.event.set_button(event.button);
        self.event.set_mods(event.mods);
        self.event.set_position(event.position);
        self.bytes.clear();
        self.encoder
            .encode_to_vec(&self.event, &mut self.bytes)
            .map_err(emulator_error)?;
        Ok(&self.bytes)
    }

    /// Forget the last reported cell, which a session does when its grid or
    /// its program changes.
    pub(crate) fn reset(&mut self) {
        self.encoder.reset();
    }
}

/// Whether the program is asking for the pointer, which is when a click
/// reports instead of starting a selection (REQ-23).
pub(crate) fn reporting(terminal: &Terminal<'static, 'static>) -> bool {
    terminal.is_mouse_tracking().unwrap_or(false)
}

/// The geometry a pixel position is converted against, with the Ghostty
/// padding included (REQ-17, REQ-23).
fn encoder_size(viewport: Viewport) -> EncoderSize {
    let padding_x = whole(viewport.padding.x);
    let padding_y = whole(viewport.padding.y);
    EncoderSize {
        screen_width: whole(f32::from(viewport.bounds.size.width)),
        screen_height: whole(f32::from(viewport.bounds.size.height)),
        cell_width: whole(viewport.cell.width).max(MIN_CELL),
        cell_height: whole(viewport.cell.height).max(MIN_CELL),
        padding_top: padding_y,
        padding_bottom: padding_y,
        padding_right: padding_x,
        padding_left: padding_x,
    }
}

/// A point measurement as the whole pixels libghostty's geometry carries.
fn whole(points: f32) -> u32 {
    if !points.is_finite() || points < 0.0 {
        return 0;
    }
    points.round() as u32
}

/// Where a window position sits inside the session, which is what both the
/// encoder and the selection gestures take.
pub(crate) fn position_in(viewport: Viewport, position: Point<Pixels>) -> Position {
    Position {
        x: f32::from(position.x - viewport.bounds.origin.x),
        y: f32::from(position.y - viewport.bounds.origin.y),
    }
}

/// The button libghostty knows a GPUI button as.
pub(crate) fn button_of(button: MouseButton) -> Option<Button> {
    match button {
        MouseButton::Left => Some(Button::Left),
        MouseButton::Right => Some(Button::Right),
        MouseButton::Middle => Some(Button::Middle),
        _ => None,
    }
}

/// One pointer event as the encoder takes it, from a GPUI event and the
/// position it falls on inside the session.
pub(crate) fn pointer_event(event: &impl Reported, position: Position) -> PointerEvent {
    PointerEvent {
        action: event.action(),
        button: event.button(),
        mods: mods_of(event.modifiers()),
        position,
    }
}

/// The facts the encoder needs of a GPUI pointer event, so one conversion
/// serves the press, the release and the move.
pub(crate) trait Reported {
    /// Press, release or motion.
    fn action(&self) -> Action;
    /// The button the event names, where it names one.
    fn button(&self) -> Option<Button>;
    /// The modifiers held down.
    fn modifiers(&self) -> Modifiers;
    /// Where the pointer is, in the window.
    fn at(&self) -> Point<Pixels>;
}

impl Reported for MouseDownEvent {
    fn action(&self) -> Action {
        Action::Press
    }

    fn button(&self) -> Option<Button> {
        button_of(self.button)
    }

    fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    fn at(&self) -> Point<Pixels> {
        self.position
    }
}

impl Reported for MouseUpEvent {
    fn action(&self) -> Action {
        Action::Release
    }

    fn button(&self) -> Option<Button> {
        button_of(self.button)
    }

    fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    fn at(&self) -> Point<Pixels> {
        self.position
    }
}

impl Reported for MouseMoveEvent {
    fn action(&self) -> Action {
        Action::Motion
    }

    fn button(&self) -> Option<Button> {
        self.pressed_button.and_then(button_of)
    }

    fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    fn at(&self) -> Point<Pixels> {
        self.position
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, MouseButton, point, px, size};
    use libghostty_vt::mouse::Button;

    use crate::metrics::{CellMetrics, Padding, Viewport};
    use crate::mouse::{button_of, encoder_size, position_in, whole};

    /// A session of 8 by 16 point cells at an offset inside the window.
    fn viewport() -> Viewport {
        Viewport {
            bounds: Bounds::new(point(px(10.), px(20.)), size(px(640.), px(400.))),
            cell: CellMetrics {
                width: 8.0,
                height: 16.0,
            },
            padding: Padding { x: 2.0, y: 2.0 },
            scale_factor: 1.0,
        }
    }

    #[test]
    fn the_geometry_carries_the_cell_and_the_padding_on_both_sides() {
        let size = encoder_size(viewport());

        assert_eq!((size.screen_width, size.screen_height), (640, 400));
        assert_eq!((size.cell_width, size.cell_height), (8, 16));
        assert_eq!((size.padding_left, size.padding_right), (2, 2));
        assert_eq!((size.padding_top, size.padding_bottom), (2, 2));
    }

    #[test]
    fn a_cell_that_rounds_to_nothing_still_has_one_pixel() {
        let tiny = Viewport {
            cell: CellMetrics {
                width: 0.2,
                height: 0.2,
            },
            ..viewport()
        };

        let size = encoder_size(tiny);

        assert_eq!((size.cell_width, size.cell_height), (1, 1));
    }

    #[test]
    fn a_window_position_is_measured_from_the_sessions_own_corner() {
        let inside = position_in(viewport(), point(px(26.), px(52.)));

        assert_eq!((inside.x, inside.y), (16.0, 32.0));
    }

    #[test]
    fn only_the_three_buttons_libghostty_reports_are_mapped() {
        assert_eq!(button_of(MouseButton::Left), Some(Button::Left));
        assert_eq!(button_of(MouseButton::Right), Some(Button::Right));
        assert_eq!(button_of(MouseButton::Middle), Some(Button::Middle));
        assert_eq!(button_of(MouseButton::Navigate(Default::default())), None);
    }

    #[test]
    fn a_measurement_that_is_not_a_size_reports_nothing() {
        assert_eq!(whole(f32::NAN), 0);
        assert_eq!(whole(-4.0), 0);
        assert_eq!(whole(7.6), 8);
    }
}
