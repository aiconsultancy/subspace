//! Starting a session: the emulator, the pseudo-terminal, the program and the
//! one reader thread that feeds the session (REQ-09, REQ-16, REQ-28).

// qual:allow(coupling, max_instability=0.84) reason: "Starting a session draws the command, the grid, the program name, the exit mapping and the error type together, and only the session module depends on it. Every dependency points at a leaf module."

use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use libghostty_vt::Terminal;
use portable_pty::{
    Child, ChildKiller, CommandBuilder, MasterPty, PtyPair, SlavePty, native_pty_system,
};

use crate::command::{self, DEFAULT_SCROLLBACK_LIMIT_BYTES, Launch};
use crate::error::{
    SessionError, emulator_error, pseudo_terminal_error, reader_error, start_error,
};
use crate::exit::{SessionExit, exit_from};
use crate::grid::SessionSize;
use crate::program::ProgramName;

/// The reader thread's buffer size.
const READ_BUFFER_BYTES: usize = 64 * 1024;

/// What the reader thread sends.
pub(crate) enum Update {
    /// Bytes read from the pseudo-terminal.
    Output(Vec<u8>),
    /// The program ended and was reaped.
    Exit(SessionExit),
}

/// The reader thread and the handles that outlive it.
struct Reader {
    output: Receiver<Update>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    pid: Option<u32>,
    exited: Arc<AtomicBool>,
}

/// Everything a started program leaves behind for its session.
pub(crate) struct Started {
    /// The emulator, which holds the screen and the scrollback.
    pub(crate) terminal: Terminal<'static, 'static>,
    /// The control end of the pseudo-terminal.
    pub(crate) master: Box<dyn MasterPty + Send>,
    /// The input end of the pseudo-terminal.
    pub(crate) writer: Box<dyn Write + Send>,
    /// The batches the reader thread sends.
    pub(crate) output: Receiver<Update>,
    /// Ends the program when a hangup does not.
    pub(crate) killer: Box<dyn ChildKiller + Send + Sync>,
    /// The program's process id, for the hangup and the foreground check.
    pub(crate) pid: Option<u32>,
    /// Set by the reader thread once the program has been reaped.
    pub(crate) exited: Arc<AtomicBool>,
}

/// Open the pseudo-terminal, start the program and start its reader thread.
pub(crate) fn start(
    plan: &Launch,
    size: SessionSize,
    program: &ProgramName,
) -> Result<Started, SessionError> {
    let terminal = create_terminal(size, plan.scrollback_limit_bytes)?;
    let PtyPair { slave, master } = open_pty(size)?;
    let child = spawn_child(slave.as_ref(), command::build_command(plan), program)?;
    drop(slave);
    let reader = start_reader(master.as_ref(), child)?;
    let writer = master.take_writer().map_err(pseudo_terminal_error)?;
    Ok(Started {
        terminal,
        master,
        writer,
        output: reader.output,
        killer: reader.killer,
        pid: reader.pid,
        exited: reader.exited,
    })
}

/// Create the emulator for a grid, with the REQ-16 scrollback limit.
fn create_terminal(
    size: SessionSize,
    scrollback_limit_bytes: Option<usize>,
) -> Result<Terminal<'static, 'static>, SessionError> {
    crate::install_logger();
    let mut terminal = Terminal::new(size.columns, size.rows).map_err(emulator_error)?;
    terminal
        .resize(
            size.columns,
            size.rows,
            size.cell_width.into(),
            size.cell_height.into(),
        )
        .map_err(emulator_error)?;
    terminal
        .set_scrollback_max_bytes(scrollback_limit(scrollback_limit_bytes))
        .map_err(emulator_error)?;
    Ok(terminal)
}

/// The scrollback limit in bytes, defaulting to Ghostty's own (REQ-16).
fn scrollback_limit(configured: Option<usize>) -> Option<usize> {
    Some(configured.unwrap_or(DEFAULT_SCROLLBACK_LIMIT_BYTES))
}

/// Open a pseudo-terminal of the given size.
fn open_pty(size: SessionSize) -> Result<PtyPair, SessionError> {
    native_pty_system()
        .openpty(size.pty_size())
        .map_err(pseudo_terminal_error)
}

/// Start the program on the pseudo-terminal's slave end.
fn spawn_child(
    slave: &dyn SlavePty,
    command: CommandBuilder,
    program: &ProgramName,
) -> Result<Box<dyn Child + Send + Sync>, SessionError> {
    slave
        .spawn_command(command)
        .map_err(|error| start_error(program, &error))
}

/// Start the waiter and reader threads and keep what outlives them.
///
/// The waiter reaps the program as soon as it ends. The reader sends the exit
/// only after the stream ends, so the REQ-09 marker follows the last output.
/// On Unix the stream ends when the program closes the terminal. A ConPTY
/// pseudo console keeps its output pipe open after the program ends, so on
/// Windows the session closes it once `exited` is set (see
/// [`crate::session`]).
fn start_reader(
    master: &dyn MasterPty,
    child: Box<dyn Child + Send + Sync>,
) -> Result<Reader, SessionError> {
    let reader = master.try_clone_reader().map_err(pseudo_terminal_error)?;
    let killer = child.clone_killer();
    let pid = child.process_id();
    let exited = Arc::new(AtomicBool::new(false));
    let (sender, output) = mpsc::channel();
    let status = spawn_waiter_thread(child, Arc::clone(&exited))?;
    spawn_reader_thread(reader, status, sender)?;
    Ok(Reader {
        output,
        killer,
        pid,
        exited,
    })
}

/// Run one waiter thread for a session, which reports how the program ended.
fn spawn_waiter_thread(
    child: Box<dyn Child + Send + Sync>,
    exited: Arc<AtomicBool>,
) -> Result<Receiver<SessionExit>, SessionError> {
    let (sender, status) = mpsc::channel();
    std::thread::Builder::new()
        .name("subspace-terminal-waiter".to_owned())
        .spawn(move || wait_for_exit(child, &exited, &sender))
        .map(|_handle| status)
        .map_err(|error| reader_error(&error))
}

/// Reap the program, then mark it gone and pass on its exit (REQ-09).
fn wait_for_exit(
    mut child: Box<dyn Child + Send + Sync>,
    exited: &AtomicBool,
    sender: &Sender<SessionExit>,
) {
    let status = child.wait();
    exited.store(true, Ordering::SeqCst);
    let _ = sender.send(exit_from(status.as_ref()));
}

/// Run one reader thread for a session.
fn spawn_reader_thread(
    reader: Box<dyn Read + Send>,
    status: Receiver<SessionExit>,
    sender: Sender<Update>,
) -> Result<(), SessionError> {
    std::thread::Builder::new()
        .name("subspace-terminal-reader".to_owned())
        .spawn(move || read_until_exit(reader, &status, &sender))
        .map(|_handle| ())
        .map_err(|error| reader_error(&error))
}

/// Read batches until the stream ends, then send the program's exit.
fn read_until_exit(
    mut reader: Box<dyn Read + Send>,
    status: &Receiver<SessionExit>,
    sender: &Sender<Update>,
) {
    send_output(reader.as_mut(), sender);
    let exit = status.recv().unwrap_or(SessionExit::Code(-1));
    let _ = sender.send(Update::Exit(exit));
}

/// Send every batch the pseudo-terminal produces until the stream ends.
///
/// Never `vt/io::to_reader`: this owns its buffer and sends only the bytes the
/// read filled (libghostty-rs issue 102).
fn send_output(reader: &mut dyn Read, sender: &Sender<Update>) {
    let mut buffer = [0u8; READ_BUFFER_BYTES];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(count) => {
                if sender
                    .send(Update::Output(buffer[..count].to_vec()))
                    .is_err()
                {
                    return;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => (),
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::command::DEFAULT_SCROLLBACK_LIMIT_BYTES;
    use crate::launch::scrollback_limit;

    #[test]
    fn the_scrollback_limit_defaults_to_the_ghostty_value() {
        assert_eq!(scrollback_limit(None), Some(DEFAULT_SCROLLBACK_LIMIT_BYTES));
    }

    #[test]
    fn a_configured_scrollback_limit_is_used_as_given() {
        assert_eq!(scrollback_limit(Some(2_048)), Some(2_048));
    }
}
