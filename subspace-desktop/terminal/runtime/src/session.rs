//! A shell session: a pseudo-terminal, a libghostty [`Terminal`] and the exit
//! or start failure the board shows (REQ-08, REQ-09, REQ-11, REQ-16, REQ-28).
//!
//! [`Terminal`] is `!Send`, so it stays on the thread that created the session.
//! One reader thread per session blocks on `read` and sends batches over a
//! channel; [`Session::drain`] feeds them to the emulator from the caller's
//! thread. No file descriptor, process id or `libc` type crosses the interface.
//!
//! On Windows the pseudo-terminal is a ConPTY pseudo console. Its output pipe
//! stays open after the program ends, so [`Session::drain_output`] closes the
//! console once the waiter thread has reaped the program; the reader then sees
//! the end of the stream and reports the exit. Closing a pseudo console can
//! wait for the console host, so it happens on a short-lived thread rather
//! than on the user-interface thread. Windows has no process groups: teardown
//! closes the console, which ends the attached programs, and kills the shell
//! if it stays.

// qual:allow(coupling, max_instability=0.86) reason: "The session is the crate's one orchestrator: it depends on the command, grid, exit, launch, environment and error modules, and only the crate root depends on it. Every dependency points at a leaf module, which is the direction the stable-dependencies principle asks for."

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use libghostty_vt::Terminal;
use portable_pty::{ChildKiller, MasterPty};

use crate::command::{self, Launch, SessionOptions};
use crate::environment::{Probe, SystemProbe};
use crate::error::{SessionError, emulator_error, input_error, pseudo_terminal_error};
use crate::exit::{SessionExit, exit_marker};
use crate::grid::SessionSize;
use crate::launch::{self, Started, Update};
use crate::program::ProgramName;

/// How many batches one [`Session::drain`] applies. A session that produces
/// more keeps the rest for the next frame, so one noisy program cannot hold
/// the user interface.
const MAX_DRAIN_BATCHES: usize = 64;

/// How long teardown waits for a hung-up shell before killing it (REQ-11).
const SHUTDOWN_GRACE: Duration = Duration::from_millis(200);

/// How often teardown checks whether the shell has gone.
const SHUTDOWN_POLL: Duration = Duration::from_millis(5);

/// The identity of a session, stable for its whole life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalId(u64);

impl TerminalId {
    /// The next identity. Unique for the life of the process.
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// What one [`Session::drain`] applied, so the caller repaints once per drain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drained {
    /// Bytes written to the emulator.
    pub bytes: usize,
    /// Whether the program ended during this drain.
    pub exited: bool,
}

impl Drained {
    /// Whether the emulator changed, which is when a repaint is worth asking
    /// for.
    pub fn changed(self) -> bool {
        self.bytes > 0 || self.exited
    }
}

/// A shell on a pseudo-terminal, with the emulator that holds its screen.
pub struct Session {
    id: TerminalId,
    program: ProgramName,
    /// The emulator, which holds the screen and the scrollback. The renderer
    /// and the input encoders live in this crate and read it directly.
    pub(crate) terminal: Terminal<'static, 'static>,
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    output: Receiver<Update>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    pid: Option<u32>,
    exited: Arc<AtomicBool>,
    exit: Option<SessionExit>,
    size: SessionSize,
}

impl Session {
    /// Start a session from the real environment and filesystem.
    pub fn start(options: &SessionOptions, size: SessionSize) -> Result<Self, SessionError> {
        Self::start_with_probe(options, size, &SystemProbe)
    }

    /// Start a session, resolving REQ-08 through an injected probe.
    pub fn start_with_probe(
        options: &SessionOptions,
        size: SessionSize,
        probe: &dyn Probe,
    ) -> Result<Self, SessionError> {
        Self::launch(&command::resolve(options, probe), size.clamped())
    }

    /// Start the program and keep what the session needs afterwards.
    fn launch(plan: &Launch, size: SessionSize) -> Result<Self, SessionError> {
        let program = plan.program.name();
        let started = launch::start(plan, size, &program)?;
        Ok(Self::new(program, started, size))
    }

    /// Assemble a session from a started program.
    fn new(program: ProgramName, started: Started, size: SessionSize) -> Self {
        Self {
            id: TerminalId::next(),
            program,
            terminal: started.terminal,
            master: Some(started.master),
            writer: Some(started.writer),
            output: started.output,
            killer: started.killer,
            pid: started.pid,
            exited: started.exited,
            exit: None,
            size,
        }
    }

    /// The session's identity.
    // qual:api (the board keys its tabs by this)
    // qual:allow(dry, boilerplate) reason: "The session owns `!Send` handles, a reader thread and its own teardown, so the three facts a board needs are read through accessors."
    pub fn id(&self) -> TerminalId {
        self.id
    }

    /// The name of the program the session started, which a tab shows until
    /// the program sets a title (REQ-07).
    // qual:api (the board titles its tabs with this)
    // qual:allow(dry, boilerplate) reason: "The session owns `!Send` handles, a reader thread and its own teardown, so the three facts a board needs are read through accessors."
    pub fn program(&self) -> &ProgramName {
        &self.program
    }

    /// The grid the session currently runs on.
    // qual:allow(dry, boilerplate) reason: "The session owns `!Send` handles, a reader thread and its own teardown, so the three facts a board needs are read through accessors."
    pub fn size(&self) -> SessionSize {
        self.size
    }

    /// How the program ended, once it has.
    pub fn exit(&self) -> Option<&SessionExit> {
        self.exit.as_ref()
    }

    /// Apply everything the reader thread has sent, then report whether a
    /// repaint is worth asking for.
    pub fn drain_output(&mut self) -> Drained {
        #[cfg(windows)]
        self.close_console_after_exit();
        let updates = self.take_updates();
        self.apply(updates)
    }

    /// Close the pseudo console once the program has been reaped, so the
    /// reader sees the end of the stream and reports the exit.
    #[cfg(windows)]
    fn close_console_after_exit(&mut self) {
        if self.exited.load(Ordering::SeqCst) && self.master.is_some() {
            self.close_pseudo_terminal();
        }
    }

    /// Send input to the program.
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), SessionError> {
        let program = self.program.clone();
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| input_error(&program, "the session has closed"))?;
        write_all(writer.as_mut(), bytes).map_err(|message| input_error(&program, &message))
    }

    /// Tell the program and the emulator about a new grid.
    pub fn resize(&mut self, size: SessionSize) -> Result<(), SessionError> {
        self.size = size.clamped();
        self.resize_pty()?;
        self.resize_terminal()
    }

    /// The program that holds the foreground, when it is not the session's own
    /// shell (REQ-10, REQ-34).
    pub fn busy_program(&self) -> Option<ProgramName> {
        let leader = foreground_leader(self.master.as_deref()?)?;
        self.foreground_name(leader)
    }

    /// The name of a foreground process group leader that is not the shell.
    fn foreground_name(&self, leader: i32) -> Option<ProgramName> {
        let leader = u32::try_from(leader).ok()?;
        match self.pid {
            Some(pid) if pid == leader => None,
            _ => crate::program::process_name(leader),
        }
    }

    /// Everything the reader thread has sent, up to one frame's worth.
    fn take_updates(&mut self) -> Vec<Update> {
        let mut updates = Vec::new();
        while updates.len() < MAX_DRAIN_BATCHES {
            match self.output.try_recv() {
                Ok(update) => updates.push(update),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        updates
    }

    /// Feed batches to the emulator and record the exit.
    fn apply(&mut self, updates: Vec<Update>) -> Drained {
        let mut drained = Drained::default();
        for update in updates {
            match update {
                Update::Output(bytes) => {
                    drained.bytes += bytes.len();
                    self.terminal.vt_write(&bytes);
                }
                Update::Exit(exit) => {
                    drained.exited = true;
                    self.terminal.vt_write(exit_marker(&exit).as_bytes());
                    self.exit = Some(exit);
                }
            }
        }
        drained
    }

    /// Report the new grid to the kernel, which signals the program.
    fn resize_pty(&self) -> Result<(), SessionError> {
        let Some(master) = self.master.as_ref() else {
            return Ok(());
        };
        master
            .resize(self.size.pty_size())
            .map_err(pseudo_terminal_error)
    }

    /// Reflow the emulator onto the new grid.
    fn resize_terminal(&mut self) -> Result<(), SessionError> {
        self.terminal
            .resize(
                self.size.columns,
                self.size.rows,
                self.size.cell_width.into(),
                self.size.cell_height.into(),
            )
            .map_err(emulator_error)
    }

    /// Close the pseudo-terminal and end the program (REQ-10, REQ-11).
    fn shutdown(&mut self) {
        self.close_pseudo_terminal();
        self.end_program();
    }

    /// Drop every handle on the pseudo-terminal.
    fn close_pseudo_terminal(&mut self) {
        drop(self.writer.take());
        if let Some(master) = self.master.take() {
            close_master(master);
        }
    }

    /// Hang up the program, then kill it if it stays.
    ///
    /// Closing the handles above is not enough on its own: the reader thread
    /// holds its own descriptor on the pseudo-terminal until it sees the end
    /// of the stream, so the hangup is sent to the process group directly.
    fn end_program(&mut self) {
        if self.exited.load(Ordering::SeqCst) {
            return;
        }
        self.hang_up();
        if wait_for_exit(&self.exited, SHUTDOWN_GRACE) {
            return;
        }
        let _ = self.killer.kill();
    }

    /// Send `SIGHUP` to the program's process group.
    #[cfg(unix)]
    fn hang_up(&self) {
        if let Some(pid) = self.process_group() {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::HUP);
        }
    }

    /// Closing the pseudo console already sent the attached programs their
    /// close event; Windows has no hangup signal to add.
    #[cfg(windows)]
    fn hang_up(&self) {}

    /// The program's process group, which is its own: the program starts in a
    /// new session, so it leads the group.
    #[cfg(unix)]
    fn process_group(&self) -> Option<rustix::process::Pid> {
        rustix::process::Pid::from_raw(i32::try_from(self.pid?).ok()?)
    }
}

/// The process group that holds the terminal's foreground.
#[cfg(unix)]
fn foreground_leader(master: &(dyn MasterPty + Send)) -> Option<i32> {
    master.process_group_leader()
}

/// A pseudo console has no foreground process group to read, so a Windows
/// session never reports a busy program and closes without asking.
#[cfg(windows)]
fn foreground_leader(_master: &(dyn MasterPty + Send)) -> Option<i32> {
    None
}

/// Close the control end of the pseudo-terminal.
#[cfg(unix)]
fn close_master(master: Box<dyn MasterPty + Send>) {
    drop(master);
}

/// Close the pseudo console away from the caller's thread: `ClosePseudoConsole`
/// can wait for the console host to flush its last output. When no thread can
/// be started, the console closes here instead.
#[cfg(windows)]
fn close_master(master: Box<dyn MasterPty + Send>) {
    let _ = std::thread::Builder::new()
        .name("subspace-terminal-close".to_owned())
        .spawn(move || drop(master));
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Session")
            .field("id", &self.id)
            .field("program", &self.program)
            .field("size", &self.size)
            .field("exit", &self.exit)
            .finish_non_exhaustive()
    }
}

/// Wait for the reader thread to report that the program has gone.
fn wait_for_exit(exited: &AtomicBool, grace: Duration) -> bool {
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if exited.load(Ordering::SeqCst) {
            return true;
        }
        std::thread::sleep(SHUTDOWN_POLL);
    }
    exited.load(Ordering::SeqCst)
}

/// Write every byte, reporting the operating system message.
fn write_all(writer: &mut dyn Write, bytes: &[u8]) -> Result<(), String> {
    writer
        .write_all(bytes)
        .and_then(|()| writer.flush())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use libghostty_vt::render::{CellIterator, RenderState, RowIterator};

    use crate::command::SessionOptions;
    use crate::environment::Probe;
    use crate::grid::SessionSize;
    use crate::session::{Drained, Session};

    /// How long a test waits for a real shell.
    const WAIT: Duration = Duration::from_secs(10);

    /// How often a test drains while waiting.
    const POLL: Duration = Duration::from_millis(10);

    /// A directory that exists on every host and is not a symbolic link, so
    /// the shell reports it unchanged as `$PWD`.
    #[cfg(unix)]
    const TEST_HOME: &str = "/usr";

    thread_local! {
        /// The replies the emulator wrote for the session under test. The
        /// view sends these in the application; a ConPTY pseudo console waits
        /// for its cursor-position query to be answered before it shows any
        /// output, so the session tests send them too. Each test runs on its
        /// own thread, so each has its own buffer.
        static REPLIES: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }

    /// A table of environment variables, with no terminfo entries, so `TERM`
    /// resolves to the fallback on every host.
    #[derive(Default)]
    struct Fake {
        variables: HashMap<String, String>,
    }

    impl Fake {
        fn with_var(mut self, name: &str, value: &str) -> Self {
            self.variables.insert(name.to_owned(), value.to_owned());
            self
        }

        /// A probe that starts `/bin/sh` as a login shell in [`TEST_HOME`].
        #[cfg(unix)]
        fn shell() -> Self {
            Self::default()
                .with_var("SHELL", "/bin/sh")
                .with_var("HOME", TEST_HOME)
        }
    }

    impl Probe for Fake {
        fn var(&self, name: &str) -> Option<String> {
            self.variables.get(name).cloned()
        }

        fn is_file(&self, _path: &Path) -> bool {
            false
        }
    }

    /// Start a session that answers the emulator's replies, failing the test
    /// with the REQ-28 message.
    fn start(options: &SessionOptions, probe: &Fake) -> Session {
        match Session::start_with_probe(options, SessionSize::default(), probe) {
            Ok(session) => answering(session),
            Err(error) => panic!("the session should start: {error}"),
        }
    }

    /// Collect the emulator's replies so [`drain_until`] can send them.
    fn answering(mut session: Session) -> Session {
        session
            .terminal
            .on_pty_write(|_terminal, bytes| {
                REPLIES.with(|replies| replies.borrow_mut().extend_from_slice(bytes));
            })
            .expect("the reply callback should install");
        session
    }

    /// Send the program the replies the last drain produced.
    fn send_replies(session: &mut Session) {
        let replies = REPLIES.with(|replies| std::mem::take(&mut *replies.borrow_mut()));
        if !replies.is_empty() {
            let _ = session.write(&replies);
        }
    }

    /// Run a command line.
    fn command_options(command: &str) -> SessionOptions {
        SessionOptions {
            command: Some(command.to_owned()),
            environment: Vec::new(),
            ..SessionOptions::default()
        }
    }

    /// The visible screen as text, one line per row.
    fn screen(session: &Session) -> String {
        let mut state = RenderState::new().expect("a render state");
        let mut rows = RowIterator::new().expect("a row iterator");
        let mut cells = CellIterator::new().expect("a cell iterator");
        let snapshot = state.update(&session.terminal).expect("a snapshot");
        let mut iteration = rows.update(&snapshot).expect("a row iteration");
        let mut text = String::new();
        while let Some(row) = iteration.next() {
            let mut row_cells = cells.update(row).expect("a cell iteration");
            let mut grapheme = String::new();
            while let Some(cell) = row_cells.next() {
                grapheme.clear();
                cell.graphemes_utf8(&mut grapheme).expect("cell text");
                text.push_str(&grapheme);
            }
            text.push('\n');
        }
        text
    }

    /// Drain until the session satisfies the condition, or the wait runs out.
    fn drain_until(session: &mut Session, ready: impl Fn(&Session) -> bool) -> bool {
        drain_within(session, WAIT, ready)
    }

    /// Drain until the session satisfies the condition, or the given wait
    /// runs out.
    fn drain_within(
        session: &mut Session,
        wait: Duration,
        ready: impl Fn(&Session) -> bool,
    ) -> bool {
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            session.drain_output();
            send_replies(session);
            if ready(session) {
                return true;
            }
            std::thread::sleep(POLL);
        }
        false
    }

    /// Wait until the condition holds, or the wait runs out.
    fn wait_until(ready: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            if ready() {
                return true;
            }
            std::thread::sleep(POLL);
        }
        false
    }

    /// Wait for the shell to show its first prompt.
    fn wait_for_prompt(session: &mut Session) {
        assert!(
            drain_until(session, |session| !screen(session).trim().is_empty()),
            "the shell should write a prompt"
        );
    }

    #[test]
    fn a_drain_that_changed_nothing_needs_no_repaint() {
        assert!(!Drained::default().changed());
        assert!(
            Drained {
                bytes: 1,
                exited: false
            }
            .changed()
        );
    }

    #[cfg(unix)]
    mod unix {
        use crate::command::SessionOptions;
        use crate::exit::SessionExit;
        use crate::grid::SessionSize;
        use crate::program::{ProgramName, process_name};
        use crate::session::Session;
        use crate::session::tests::{
            Fake, TEST_HOME, command_options, drain_until, screen, start, wait_for_prompt,
            wait_until,
        };

        /// The screen lines a program produced: the echoed input still holds the
        /// unexpanded `$` names, so those lines are not output.
        fn output_lines(session: &Session) -> Vec<String> {
            screen(session)
                .lines()
                .filter(|line| !line.contains('$'))
                .map(|line| line.trim().to_owned())
                .filter(|line| !line.is_empty())
                .collect()
        }

        #[test]
        fn child_environments_are_isolated_between_real_pty_sessions() {
            let original = std::env::var_os("SUBSPACE_PTY_ENV_TEST");
            for value in ["first-profile", "second-profile"] {
                let mut options = command_options(
                    "/bin/sh -c 'printf \"profile=%s\\n\" \"$SUBSPACE_PTY_ENV_TEST\"'",
                );
                options.environment = vec![("SUBSPACE_PTY_ENV_TEST".into(), value.into())];
                let mut session = start(&options, &Fake::shell());
                let expected = format!("profile={value}");
                assert!(drain_until(&mut session, |session| screen(session).contains(&expected)));
            }
            assert_eq!(std::env::var_os("SUBSPACE_PTY_ENV_TEST"), original);
        }

        #[test]
        fn a_login_shell_reports_the_req_08_environment() {
            let mut session = start(&SessionOptions::default(), &Fake::shell());
            assert_eq!(session.program(), &ProgramName::from_path("sh"));
            wait_for_prompt(&mut session);

            session
                .write(b"echo $0 $PWD $TERM $COLORTERM\n")
                .expect("the shell should accept input");
            let reported = drain_until(&mut session, |session| {
                output_lines(session)
                    .iter()
                    .any(|line| line.contains("truecolor"))
            });

            assert!(reported, "the shell should report its environment");
            let line = output_lines(&session)
                .into_iter()
                .find(|line| line.contains("truecolor"))
                .unwrap_or_default();
            assert_eq!(line, format!("-sh {TEST_HOME} xterm-256color truecolor"));
        }

        /// REQ-08 asks that `infocmp $TERM` succeed inside the session, so this
        /// runs the real resolution against the host: `xterm-ghostty` where a
        /// terminfo entry exists, `xterm-256color` where none does.
        #[test]
        fn the_resolved_term_resolves_inside_the_session() {
            let report = "if command -v infocmp >/dev/null; then \
             infocmp \"$TERM\" >/dev/null 2>&1 && echo terminfo=ok || echo terminfo=missing; \
             else echo terminfo=skipped; fi";
            let options = SessionOptions {
                command: Some(format!("/bin/sh -c '{report}'")),
                ..SessionOptions::default()
            };
            let mut session = match Session::start(&options, SessionSize::default()) {
                Ok(session) => session,
                Err(error) => panic!("the session should start: {error}"),
            };

            let reported = drain_until(&mut session, |session| {
                output_lines(session)
                    .iter()
                    .any(|line| line.starts_with("terminfo="))
            });

            assert!(reported, "the session should report its terminfo lookup");
            let line = output_lines(&session)
                .into_iter()
                .find(|line| line.starts_with("terminfo="))
                .unwrap_or_default();
            assert_ne!(
                line, "terminfo=missing",
                "the resolved TERM should have a terminfo entry the session can read"
            );
        }

        #[test]
        fn an_exit_code_ends_the_session_and_marks_the_screen() {
            let mut session = start(&command_options("/bin/sh -c 'exit 3'"), &Fake::default());

            let exited = drain_until(&mut session, |session| session.exit().is_some());

            assert!(exited, "the program should end");
            assert_eq!(session.exit(), Some(&SessionExit::Code(3)));
            assert!(
                screen(&session).contains("[process exited with code 3]"),
                "the screen should carry the REQ-09 marker: {:?}",
                screen(&session)
            );
        }

        #[test]
        fn a_signalled_program_reports_the_signal() {
            let mut session = start(
                &command_options("/bin/sh -c 'kill -TERM $$'"),
                &Fake::default(),
            );

            let exited = drain_until(&mut session, |session| session.exit().is_some());

            assert!(exited, "the program should end");
            match session.exit() {
                Some(SessionExit::Signal(signal)) => {
                    assert!(
                        signal.to_lowercase().contains("term"),
                        "the signal should be named: {signal:?}"
                    );
                }
                other => panic!("the exit should name a signal, not {other:?}"),
            }
        }

        #[test]
        fn a_missing_shell_fails_by_name_and_a_corrected_one_starts() {
            let broken = Fake::default()
                .with_var("SHELL", "/missing/shell")
                .with_var("HOME", TEST_HOME);

            let error = Session::start_with_probe(
                &SessionOptions::default(),
                SessionSize::default(),
                &broken,
            )
            .expect_err("a missing shell should not start");

            let message = error.to_string();
            assert!(
                message.contains("/missing/shell"),
                "the failure should name the shell: {message}"
            );
            assert!(
                message.to_lowercase().contains("no such file"),
                "the failure should carry the operating-system message: {message}"
            );
            assert_eq!(error.program(), Some(&ProgramName::from_path("shell")));

            let session = start(&SessionOptions::default(), &Fake::shell());
            assert_eq!(session.program(), &ProgramName::from_path("sh"));
        }

        #[test]
        fn resizing_reaches_the_program_and_the_emulator() {
            let mut session = start(&SessionOptions::default(), &Fake::shell());
            wait_for_prompt(&mut session);

            session
                .resize(SessionSize {
                    columns: 100,
                    rows: 30,
                    ..SessionSize::default()
                })
                .expect("the session should resize");
            session
                .write(b"stty size\n")
                .expect("the shell should accept input");
            let reported = drain_until(&mut session, |session| {
                output_lines(session).iter().any(|line| line == "30 100")
            });

            assert!(reported, "the program should see 30 rows and 100 columns");
            assert_eq!(session.terminal.cols().expect("the emulator width"), 100);
            assert_eq!(session.size().rows, 30);
        }

        #[test]
        fn the_foreground_program_is_named_while_it_runs() {
            let mut session = start(&SessionOptions::default(), &Fake::shell());
            wait_for_prompt(&mut session);
            assert_eq!(
                session.busy_program(),
                None,
                "a shell at its prompt is not busy"
            );

            session
                .write(b"sleep 30\n")
                .expect("the shell should accept input");
            let busy = drain_until(&mut session, |session| session.busy_program().is_some());

            assert!(busy, "the foreground program should be named");
            assert_eq!(
                session.busy_program(),
                Some(ProgramName::from_path("sleep"))
            );
        }

        #[test]
        fn dropping_a_session_ends_its_program() {
            let mut session = start(&SessionOptions::default(), &Fake::shell());
            wait_for_prompt(&mut session);
            let pid = session.pid.expect("the shell should have a process id");

            drop(session);

            assert!(
                wait_until(|| process_name(pid).is_none()),
                "the shell should be gone once the session is dropped"
            );
        }
    }

    /// Sessions on a ConPTY pseudo console. `/d` stops `cmd.exe` running the
    /// user's AutoRun commands, so the output is the same on every host.
    #[cfg(windows)]
    mod windows {
        use crate::command::SessionOptions;
        use crate::exit::SessionExit;
        use crate::grid::SessionSize;
        use crate::program::ProgramName;
        use crate::session::Session;
        use std::time::Duration;

        use crate::session::tests::{
            Fake, answering, command_options, drain_until, drain_within, screen, start,
            wait_for_prompt, wait_until,
        };

        /// An interactive `cmd.exe` that does not echo its commands.
        const INTERACTIVE: &str = "cmd.exe /d /q /k";

        /// How long the real default shell may take to show its prompt. A cold
        /// PowerShell start that loads the user's profile can take more than
        /// ten seconds on a busy runner.
        const SHELL_START_WAIT: Duration = Duration::from_secs(60);

        /// Whether a process with this id is still running.
        fn running(pid: u32) -> bool {
            let filter = format!("PID eq {pid}");
            std::process::Command::new("tasklist")
                .args(["/FI", &filter, "/NH", "/FO", "CSV"])
                .output()
                .map(|output| {
                    String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
                })
                .unwrap_or(false)
        }

        #[test]
        fn a_child_environment_reaches_a_conpty_session() {
            let mut options = command_options("cmd.exe /d /c echo profile=%SUBSPACE_PTY_ENV_TEST%");
            options.environment = vec![("SUBSPACE_PTY_ENV_TEST".into(), "windows-profile".into())];

            let mut session = start(&options, &Fake::default());

            assert!(
                drain_until(&mut session, |session| screen(session)
                    .contains("profile=windows-profile")),
                "the program should see the session variable: {:?}",
                screen(&session)
            );
            assert_eq!(std::env::var_os("SUBSPACE_PTY_ENV_TEST"), None);
        }

        /// The pseudo console keeps its output open after the program ends;
        /// the session must still report the exit and mark the screen.
        #[test]
        fn an_exit_code_ends_a_conpty_session_and_marks_the_screen() {
            let mut session = start(&command_options("cmd.exe /d /c exit 3"), &Fake::default());

            let exited = drain_until(&mut session, |session| session.exit().is_some());

            assert!(exited, "the program should end");
            assert_eq!(session.exit(), Some(&SessionExit::Code(3)));
            assert!(
                screen(&session).contains("[process exited with code 3]"),
                "the screen should carry the REQ-09 marker: {:?}",
                screen(&session)
            );
        }

        #[test]
        fn output_written_just_before_the_exit_is_kept() {
            let mut session = start(
                &command_options("cmd.exe /d /c \"echo last-words& exit 4\""),
                &Fake::default(),
            );

            let exited = drain_until(&mut session, |session| session.exit().is_some());

            assert!(exited, "the program should end");
            let text = screen(&session);
            let words = text.find("last-words");
            let marker = text.find("[process exited with code 4]");
            assert!(
                words.is_some() && marker.is_some() && words < marker,
                "the last output should come before the exit marker: {text:?}"
            );
        }

        #[test]
        fn a_missing_shell_fails_by_name() {
            let broken = Fake::default().with_var("SHELL", r"C:\missing\shell.exe");

            let error = Session::start_with_probe(
                &SessionOptions::default(),
                SessionSize::default(),
                &broken,
            )
            .expect_err("a missing shell should not start");

            assert_eq!(error.program(), Some(&ProgramName::from_path("shell.exe")));
            assert!(
                error.to_string().contains("shell.exe"),
                "the failure should name the shell: {error}"
            );
        }

        #[test]
        fn resizing_reaches_a_conpty_program_and_the_emulator() {
            let mut session = start(&command_options(INTERACTIVE), &Fake::default());
            wait_for_prompt(&mut session);

            session
                .resize(SessionSize {
                    columns: 100,
                    rows: 30,
                    ..SessionSize::default()
                })
                .expect("the session should resize");
            session
                .write(b"mode con\r")
                .expect("the shell should accept input");
            let reported = drain_until(&mut session, |session| {
                screen(session)
                    .lines()
                    .any(|line| line.contains("Columns:") && line.trim_end().ends_with("100"))
            });

            assert!(
                reported,
                "the program should see 100 columns: {:?}",
                screen(&session)
            );
            assert_eq!(session.terminal.cols().expect("the emulator width"), 100);
        }

        #[test]
        fn dropping_a_conpty_session_ends_its_program() {
            let mut session = start(&command_options(INTERACTIVE), &Fake::default());
            wait_for_prompt(&mut session);
            let pid = session.pid.expect("the shell should have a process id");
            assert!(running(pid), "the shell should run before the drop");

            drop(session);

            assert!(
                wait_until(|| !running(pid)),
                "the shell should be gone once the session is dropped"
            );
        }

        /// The real default: PowerShell when it is installed, else `ComSpec`.
        #[test]
        fn the_default_windows_shell_starts_and_prompts() {
            let session = Session::start(&SessionOptions::default(), SessionSize::default())
                .expect("the default shell should start");
            let mut session = answering(session);
            let name = session.program().as_str().to_ascii_lowercase();

            assert!(
                ["pwsh.exe", "powershell.exe", "cmd.exe"].contains(&name.as_str()),
                "the default should be PowerShell or cmd.exe: {name}"
            );
            assert!(
                drain_within(&mut session, SHELL_START_WAIT, |session| {
                    !screen(session).trim().is_empty()
                }),
                "{name} should write a prompt within {SHELL_START_WAIT:?}"
            );
        }
    }
}
