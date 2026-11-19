//! The clipboard sequences a program sends, and the replies it reads back
//! (REQ-24).
//!
//! libghostty normalises OSC 52 and its relatives into two callbacks, and
//! both have to answer before they return, while the VT stream is paused. A
//! session's clipboard, though, belongs to GPUI and is only reachable from
//! the foreground with an `App`. The two meet in [`ClipboardEffects`], a
//! cell the callbacks and the view share:
//!
//! - A **write** the policy allows is granted at once and the text queued.
//!   The view takes the queue after every drain and puts it on the real
//!   clipboard. OSC 52 discards the reply anyway, so nothing is lost by
//!   answering before the clipboard is set.
//! - A **read** is answered from [`ClipboardEffects::offered`], which the
//!   view fills from the real clipboard before each drain and only while
//!   the policy allows reads. A refused read replies with nothing at all,
//!   so no clipboard content is exposed (REQ-24).
//! - The **reply bytes** libghostty writes for a read, and for every other
//!   query a program makes, are queued the same way and written to the
//!   pseudo-terminal after the drain: the callback may not touch the session
//!   it was called from.
//!
//! The callbacks never keep the cell borrowed across a reply, because a
//! reply runs the pty-write callback inside it and a second borrow would
//! panic (libghostty-rs documents this, and a panic in a callback aborts).
//! No desktop-notification callback is installed (libghostty-rs issue 101).

use std::cell::RefCell;
use std::rc::Rc;

use libghostty_vt::Terminal;
use libghostty_vt::terminal::{
    ClipboardLocation, ClipboardRead, ClipboardReadError, ClipboardReplyContent, ClipboardWrite,
};

use crate::clipboard::TerminalClipboard;
use crate::error::{SessionError, emulator_error};

/// The MIME type OSC 52 carries.
const TEXT_PLAIN: &str = "text/plain";

/// One clipboard write a program asked for, waiting for the foreground.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClipboardWriteRequest {
    /// Which clipboard the program named.
    pub(crate) primary: bool,
    /// The text to put on it.
    pub(crate) text: String,
}

/// What a session's clipboard callbacks and its view share.
#[derive(Default)]
pub(crate) struct ClipboardEffects {
    /// The policy both callbacks answer by, which a reload replaces.
    policy: TerminalClipboard,
    /// The clipboard text a program may read, which the view fills before a
    /// drain and only while the policy allows reads.
    offered: Option<String>,
    /// The writes a program asked for, for the view to apply.
    writes: Vec<ClipboardWriteRequest>,
    /// The bytes libghostty wrote for the program, for the view to send.
    replies: Vec<u8>,
}

impl ClipboardEffects {
    /// The shared cell for one session.
    pub(crate) fn new(policy: TerminalClipboard) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            policy,
            ..Self::default()
        }))
    }

    /// Answer by a new policy, which a configuration reload brings
    /// (REQ-33).
    pub(crate) fn restyle(&mut self, policy: TerminalClipboard) {
        self.policy = policy;
        self.offered = None;
    }

    /// Whether a program may read the clipboard at all, which decides
    /// whether the view reads the real one before a drain (REQ-24).
    pub(crate) fn offers_reads(&self) -> bool {
        self.policy.allows_program_read()
    }

    /// Hold the clipboard text a program may read during the next drain.
    pub(crate) fn offer(&mut self, text: Option<String>) {
        self.offered = text;
    }

    /// Take the writes a program asked for.
    pub(crate) fn take_writes(&mut self) -> Vec<ClipboardWriteRequest> {
        std::mem::take(&mut self.writes)
    }

    /// Take the bytes libghostty wrote for the program.
    pub(crate) fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies)
    }
}

/// Install the three callbacks one session needs, so a program's clipboard
/// sequences and query replies reach the view (REQ-24).
pub(crate) fn install(
    terminal: &mut Terminal<'static, 'static>,
    effects: &Rc<RefCell<ClipboardEffects>>,
) -> Result<(), SessionError> {
    let writes = Rc::clone(effects);
    terminal
        .on_clipboard_write(move |_terminal, request| answer_write(&writes, request))
        .map_err(emulator_error)?;
    let reads = Rc::clone(effects);
    terminal
        .on_clipboard_read(move |_terminal, request| answer_read(&reads, request))
        .map_err(emulator_error)?;
    let replies = Rc::clone(effects);
    terminal
        .on_pty_write(move |_terminal, bytes| {
            replies.borrow_mut().replies.extend_from_slice(bytes);
        })
        .map_err(emulator_error)?;
    Ok(())
}

/// Answer a program's clipboard write: queue the text for the view when the
/// policy allows it, and deny it otherwise (REQ-24).
fn answer_write(effects: &Rc<RefCell<ClipboardEffects>>, request: ClipboardWrite<'_>) {
    let queued = queue_write(effects, &request);
    // The borrow is over: a reply may run the pty-write callback, which
    // borrows the same cell.
    match queued {
        true => request.reply(Ok(()), false),
        false => request.reply(
            Err(libghostty_vt::terminal::ClipboardWriteError::Denied),
            false,
        ),
    }
}

/// Queue the text of an allowed write, and report whether it was allowed.
fn queue_write(effects: &Rc<RefCell<ClipboardEffects>>, request: &ClipboardWrite<'_>) -> bool {
    let mut effects = effects.borrow_mut();
    if !effects.policy.allows_program_write() {
        return false;
    }
    let Some(text) = written_text(request) else {
        return false;
    };
    effects.writes.push(ClipboardWriteRequest {
        primary: is_primary(request.location()),
        text,
    });
    true
}

/// The text of a write's first text representation, and nothing for a write
/// carrying no text, which includes the empty write that clears a
/// clipboard.
fn written_text(request: &ClipboardWrite<'_>) -> Option<String> {
    request
        .contents()
        .filter(|content| content.mime.starts_with("text/"))
        .map(|content| String::from_utf8_lossy(content.data).into_owned())
        .next()
}

/// Answer a program's clipboard read: the offered text when the policy
/// allows reads, and nothing at all otherwise (REQ-24).
fn answer_read(effects: &Rc<RefCell<ClipboardEffects>>, request: ClipboardRead<'_>) {
    let offered = read_answer(effects);
    // The borrow is over before the reply, which writes to the
    // pseudo-terminal through the pty-write callback.
    let Some(text) = offered else {
        request.reply(Err(ClipboardReadError::Denied), &[], false);
        return;
    };
    let contents = [ClipboardReplyContent::new(TEXT_PLAIN, text.as_bytes())];
    request.reply(Ok(&contents), &[], false);
}

/// The text a read is answered with, and nothing for a refused read.
fn read_answer(effects: &Rc<RefCell<ClipboardEffects>>) -> Option<String> {
    let effects = effects.borrow();
    if !effects.policy.allows_program_read() {
        return None;
    }
    effects.offered.clone()
}

/// Whether a location names the primary selection, which only Linux has.
fn is_primary(location: ClipboardLocation) -> bool {
    matches!(
        location,
        ClipboardLocation::Primary | ClipboardLocation::Selection
    )
}

#[cfg(test)]
mod tests {
    use libghostty_vt::Terminal;
    use subspace_ghostty_config::{ClipboardAccess, GhosttyConfig};

    use crate::clipboard::TerminalClipboard;
    use crate::effects::{ClipboardEffects, install};

    /// The clipboard policy of a configuration with the given access.
    fn policy(read: ClipboardAccess, write: ClipboardAccess) -> TerminalClipboard {
        TerminalClipboard::from_config(&GhosttyConfig {
            clipboard_read: read,
            clipboard_write: write,
            ..GhosttyConfig::default()
        })
    }

    /// A terminal with the clipboard callbacks installed.
    fn terminal(
        policy: TerminalClipboard,
    ) -> (
        Terminal<'static, 'static>,
        std::rc::Rc<std::cell::RefCell<ClipboardEffects>>,
    ) {
        let mut terminal = Terminal::new(20, 3).expect("a terminal");
        let effects = ClipboardEffects::new(policy);
        install(&mut terminal, &effects).expect("the callbacks should install");
        (terminal, effects)
    }

    /// An OSC 52 write of `aGk=`, which is `hi`.
    const WRITE_HI: &[u8] = b"\x1b]52;c;aGk=\x07";

    /// An OSC 52 read request.
    const READ: &[u8] = b"\x1b]52;c;?\x07";

    #[test]
    fn an_allowed_write_is_queued_for_the_view() {
        let (mut terminal, effects) = terminal(TerminalClipboard::default());

        terminal.vt_write(WRITE_HI);
        let writes = effects.borrow_mut().take_writes();

        assert_eq!(writes.len(), 1, "{writes:?}");
        assert_eq!(writes[0].text, "hi");
        assert!(!writes[0].primary, "OSC 52 c is the system clipboard");
    }

    #[test]
    fn a_denied_or_asked_write_reaches_no_clipboard() {
        for access in [ClipboardAccess::Deny, ClipboardAccess::Ask] {
            let (mut terminal, effects) = terminal(policy(ClipboardAccess::Ask, access));

            terminal.vt_write(WRITE_HI);

            assert!(
                effects.borrow_mut().take_writes().is_empty(),
                "{access:?} should write nothing"
            );
        }
    }

    #[test]
    fn a_read_is_refused_with_no_reply_and_no_content() {
        let (mut terminal, effects) = terminal(TerminalClipboard::default());
        effects.borrow_mut().offer(Some("secret".to_owned()));

        terminal.vt_write(READ);
        let replies = effects.borrow_mut().take_replies();

        assert!(
            !String::from_utf8_lossy(&replies).contains("secret"),
            "the clipboard must not be exposed: {replies:?}"
        );
    }

    #[test]
    fn a_read_is_answered_only_when_the_configuration_allows_it() {
        let (mut terminal, effects) =
            terminal(policy(ClipboardAccess::Allow, ClipboardAccess::Allow));
        effects.borrow_mut().offer(Some("hi".to_owned()));
        assert!(effects.borrow().offers_reads());

        terminal.vt_write(READ);
        let replies = effects.borrow_mut().take_replies();

        assert!(
            String::from_utf8_lossy(&replies).contains("aGk="),
            "the reply should carry the clipboard: {replies:?}"
        );
    }

    #[test]
    fn a_reload_replaces_the_policy_and_drops_the_offered_text() {
        let (mut terminal, effects) =
            terminal(policy(ClipboardAccess::Allow, ClipboardAccess::Allow));
        effects.borrow_mut().offer(Some("hi".to_owned()));

        effects
            .borrow_mut()
            .restyle(policy(ClipboardAccess::Deny, ClipboardAccess::Deny));
        terminal.vt_write(READ);
        terminal.vt_write(WRITE_HI);

        assert!(!effects.borrow().offers_reads());
        assert!(effects.borrow_mut().take_writes().is_empty());
        let replies = effects.borrow_mut().take_replies();
        assert!(
            !String::from_utf8_lossy(&replies).contains("aGk="),
            "{replies:?}"
        );
    }
}
