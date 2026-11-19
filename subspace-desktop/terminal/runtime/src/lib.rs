//! Shell sessions for the Subspace terminal panel.
//!
//! A [`Session`] runs the user's shell on a pseudo-terminal and feeds its
//! output into a libghostty [`Terminal`](libghostty_vt::Terminal). The
//! emulator is `!Send`, so it stays on the thread that created the session:
//! one reader thread per session sends batches of bytes over a channel, and
//! the owning thread calls [`Session::drain_output`] to apply them.
//!
//! The crate is the only one in the workspace that needs Zig, so it sits
//! outside the workspace members. It runs on a Unix pseudo-terminal and, on
//! Windows, on a ConPTY pseudo console (see [`session`]).
//!
//! # Drawing a session
//!
//! [`TerminalView`] is the GPUI entity the terminal panel embeds, one per tab. It owns
//! one session, the render state that reads it and the [`TerminalStyle`] it
//! draws with, and its custom element draws the grid, the attributes, the
//! cursor and the scrollbar (REQ-12 to REQ-17). It also sends the program its
//! keys and its input-method commits (REQ-18, REQ-31a), selects text with the
//! pointer or reports the pointer to the program (REQ-23), and carries copy,
//! paste and the program's own clipboard sequences between the session and
//! the system clipboard (REQ-22, REQ-24). The panel resolves the style from
//! the Ghostty configuration once and passes it through:
//!
//! ```ignore
//! let (style, warnings) = TerminalStyle::resolve(&config, &theme, cx);
//! let view = TerminalView::open(&options, style, cx)?;
//! ```
//!
//! The panel also calls [`bind_keys`] once, from its registration's
//! `bind_keys`, which binds copy and paste in the session's key
//! context (REQ-19, REQ-20, REQ-22).
//!
//! # REQ-08 and REQ-28
//!
//! REQ-08 replaces an unset or empty `SHELL` with `/bin/sh`. On Windows it is
//! replaced with `pwsh.exe` or `powershell.exe` from `PATH`, then `ComSpec`,
//! then `cmd.exe`. A `SHELL` that
//! names a missing or non-executable file is kept instead of replaced, because
//! REQ-28 requires a start failure that names the shell the user configured;
//! substituting a working shell would hide the broken configuration. See
//! [`program::resolve_program`].
//!
//! # libghostty soundness rules
//!
//! `libghostty-vt` is pre-1.0. Five open issues were re-checked at the pinned
//! rev `d2036ba`; each is contained by a rule this crate keeps:
//!
//! | Issue | Rule |
//! | --- | --- |
//! | 70, `graphemes_buf` passes no capacity | Never call it. Use `graphemes_len` and `graphemes_utf8`. `clippy.toml` bans the method. |
//! | 101, `DesktopNotification::title` and `body` | Never use the desktop-notification API. |
//! | 102, `vt/io::to_reader` exposes uninitialised memory | Never use it. The reader thread owns its buffer and writes only the bytes a read filled. |
//! | 103, `From<A> for Allocator` dangles | Never enable the `allocator_api` feature. |
//! | 104, `set_logger` and `set_png_decoder` race | [`install_logger`] runs once, before any terminal exists, and never again. No PNG decoder: `default-features = false` removes the Kitty graphics module. |

pub mod clipboard;
pub mod close_policy;
mod command;
pub mod config;
mod effects;
mod element;
pub mod environment;
pub mod error;
pub mod exit;
mod font;
mod frame;
pub mod grid;
pub mod input;
mod launch;
pub mod metrics;
mod mouse;
pub mod palette;
mod pointer;
pub mod program;
mod scroll;
mod selection;
pub mod session;
pub mod settings;
mod snapshot;
mod sprite;
mod state;
pub mod style;
pub mod tabs;
mod typing;
pub mod view;
#[cfg(test)]
mod view_tests;

pub use clipboard::{CopySelection, PasteClipboard, TerminalClipboard};
pub use command::{DEFAULT_SCROLLBACK_LIMIT_BYTES, SessionOptions};
pub use environment::{
    COLORTERM, FALLBACK_TERM, GHOSTTY_TERM, Probe, SystemProbe, TermSelection, resolve_term,
    resolve_working_directory,
};
pub use error::SessionError;
pub use exit::SessionExit;
pub use grid::SessionSize;
pub use input::TerminalKeybinds;
pub use metrics::{CellMetrics, Padding};
pub use palette::{ANSI_COLORS, TerminalPalette};
pub use program::{DEFAULT_SHELL, Program, ProgramName, resolve_program};
pub use session::{Drained, Session, TerminalId};
pub use style::{CursorDefaults, TerminalFonts, TerminalStyle};
pub use view::{TERMINAL_CONTEXT, TerminalView};

/// Bind the session's own keys, which the terminal panel calls from its
/// registration's `bind_keys`: copy and paste in the session's key context
/// (REQ-19, REQ-20, REQ-22).
///
/// Every other key a focused session receives goes to the program, so this
/// is the whole of the crate's keymap.
// qual:api (the terminal panel calls this from its own `bind_keys`)
pub fn bind_keys(cx: &mut gpui_kit::App) {
    clipboard::bind_keys(view::TERMINAL_CONTEXT, cx);
}

/// Install the libghostty log callback, once per process.
///
/// libghostty-rs issue 104: `set_logger` races with live terminals, so it runs
/// before the first [`Session`] creates one and never again. Messages go to
/// standard error, where the rest of the application's diagnostics go.
pub fn install_logger() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let _ = libghostty_vt::set_logger(Some(Box::new(libghostty_vt::log::LogStderr)));
    });
}
