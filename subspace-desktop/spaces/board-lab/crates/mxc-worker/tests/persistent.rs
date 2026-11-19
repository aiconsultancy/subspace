use subspace_mxc_worker::{PersistentProcess, SpawnRequest, spawn};

fn require_send<T: Send>() {}

#[test]
fn request_debug_never_discloses_arguments_proxy_or_delegated_values() {
    require_send::<PersistentProcess>();
    let mut request = SpawnRequest::new(
        "/bin/sh".into(),
        vec!["/runtime".into()],
        "/workspace".into(),
    );
    request.args = vec!["dummy-test-only-argument".into()];
    request.proxy = Some("dummy-test-only-proxy".into());
    request.environment = vec![("API_TOKEN".into(), "dummy-test-only-value".into())];
    let debug = format!("{request:?}");
    assert!(!debug.contains("dummy-test-only"));
}

#[test]
fn invalid_delegation_is_rejected_before_any_process_or_path_lookup() {
    for name in ["HOME", "INVALID-NAME"] {
        let mut request = SpawnRequest::new(
            "/nonexistent-executable".into(),
            Vec::new(),
            "/nonexistent-workspace".into(),
        );
        request.environment = vec![(name.into(), "dummy-test-only-value".into())];
        let error = spawn(request).unwrap_err();
        assert!(error.to_string().contains("environment name"));
        assert!(!format!("{error:?}").contains("dummy-test-only-value"));
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
#[test]
fn persistent_spawn_fails_closed_on_unsupported_platforms() {
    let request = SpawnRequest::new("/bin/echo".into(), vec!["/bin".into()], "/workspace".into());
    let error = spawn(request).unwrap_err();
    assert!(error.to_string().contains("unsupported"));
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod contained {
    use super::{SpawnRequest, spawn};
    use std::io::{self, BufRead, BufReader, Read, Write};
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    type Output = BufReader<Box<dyn Read + Send>>;

    struct Workspace(PathBuf);

    impl Workspace {
        fn new() -> Self {
            use std::os::unix::fs::DirBuilderExt;
            static NEXT: AtomicU64 = AtomicU64::new(0);
            for _ in 0..1000 {
                let nonce = NEXT.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir()
                    .join(format!("subspace-mxc-test-{}-{nonce}", std::process::id()));
                match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                    Ok(()) => return Self(std::fs::canonicalize(path).unwrap()),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("failed to create test workspace: {error}"),
                }
            }
            panic!("could not find an unused test workspace path")
        }

        fn request(&self, script: &str) -> SpawnRequest {
            let mut request =
                SpawnRequest::new("/bin/sh".into(), vec!["/bin".into()], self.0.clone());
            request.args = vec!["-c".into(), script.into()];
            request
        }
    }

    impl Drop for Workspace {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn parallel_workspaces_are_distinct_private_and_removed_on_drop() {
        use std::os::unix::fs::PermissionsExt;

        let threads: Vec<_> = (0..64)
            .map(|_| std::thread::spawn(Workspace::new))
            .collect();
        let workspaces: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        let paths: std::collections::HashSet<_> = workspaces
            .iter()
            .map(|workspace| workspace.0.clone())
            .collect();
        assert_eq!(paths.len(), workspaces.len());
        for path in &paths {
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(*path, std::fs::canonicalize(path).unwrap());
        }
        drop(workspaces);
        assert!(paths.iter().all(|path| !path.exists()));
    }

    fn line(reader: Output) -> (Output, String) {
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            let mut reader = reader;
            let mut text = String::new();
            let count = reader.read_line(&mut text);
            let _ = sender.send((reader, text, count));
        });
        let (reader, text, count) = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(thread.join().is_ok());
        assert!(count.unwrap() > 0, "contained output ended before a line");
        (reader, text)
    }

    fn parked_reader(
        mut output: impl Read + Send + 'static,
        input: &mut dyn Write,
    ) -> (
        mpsc::Receiver<io::Result<usize>>,
        std::thread::JoinHandle<()>,
    ) {
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let (sender, receiver) = mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            let mut marker = [0];
            let progressed = output.read_exact(&mut marker).map(|()| marker[0]);
            let _ = ready_sender.send(progressed);
            let _ = sender.send(output.read(&mut [0]));
        });
        // The fixture cannot emit the marker until this handshake. Consuming it
        // proves actual pipe read progress, not merely that the thread started.
        input.write_all(b"readiness\n").unwrap();
        input.flush().unwrap();
        assert_eq!(
            ready_receiver
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .unwrap(),
            b'R',
        );
        // Keep the worker alive and silent. No EOF or completed read may precede
        // cancellation; this is a bounded channel assertion, not a sleep.
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout),
        ));
        (receiver, reader)
    }

    #[test]
    fn contained_session_retains_state_and_only_selected_environment() {
        let workspace = Workspace::new();
        let script = "printf 'diagnostic\\n' >&2; count=0; while IFS= read -r input; do count=$((count+1)); printf '%s|%s|%s|%s|%s\\n' \"$count\" \"$input\" \"$SELECTED_TEST_VALUE\" \"${USER-unset}\" \"$HOME\"; done";
        let mut request = workspace.request(script);
        request.environment = vec![(
            "SELECTED_TEST_VALUE".into(),
            "dummy-test-only; $(not-executed)".into(),
        )];
        let mut process = spawn(request).unwrap();
        let pid = process.id();
        let mut input = process.take_stdin().unwrap();
        let output = BufReader::new(process.take_stdout().unwrap());
        let diagnostics = BufReader::new(process.take_stderr().unwrap());
        assert_eq!(line(diagnostics).1, "diagnostic\n");
        input.write_all(b"first\n").unwrap();
        input.flush().unwrap();
        let (output, first) = line(output);
        assert_eq!(
            first,
            format!(
                "1|first|dummy-test-only; $(not-executed)|unset|{}\n",
                workspace.0.display()
            )
        );
        input.write_all(b"second\n").unwrap();
        input.flush().unwrap();
        assert!(line(output).1.starts_with("2|second|"));
        assert_eq!(process.id(), pid);
        assert_eq!(process.try_wait().unwrap(), None);
        drop(input);
        assert_eq!(process.wait().unwrap(), 0);
    }

    #[test]
    fn stop_reaps_the_worker_and_ordinary_descendants_and_unblocks_output() {
        let workspace = Workspace::new();
        let mut process = spawn(workspace.request(
            "/bin/sleep 120 & printf '%s\\n' \"$!\"; IFS= read -r signal; printf R; wait",
        ))
        .unwrap();
        let pid = process.id();
        let (output, child) = line(BufReader::new(process.take_stdout().unwrap()));
        let child: u32 = child.trim().parse().unwrap();
        let mut input = process.take_stdin().unwrap();
        let (receiver, reader) = parked_reader(output, &mut *input);
        assert!(process.stop().is_ok());
        assert!(process.try_wait().unwrap().is_some());
        assert!(
            receiver
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .is_ok()
        );
        assert!(reader.join().is_ok());
        assert!(
            !Command::new("/bin/kill")
                .args(["-0", &pid.to_string()])
                .output()
                .unwrap()
                .status
                .success()
        );
        let status = Command::new("/bin/ps")
            .args(["-p", &child.to_string(), "-o", "stat="])
            .output()
            .unwrap();
        let status = String::from_utf8(status.stdout).unwrap();
        assert!(
            status.trim().is_empty() || status.trim().starts_with('Z'),
            "descendant still running: {status}"
        );
    }

    #[test]
    fn output_cancellation_unblocks_readers_without_stopping_the_worker() {
        let workspace = Workspace::new();
        let mut process =
            spawn(workspace.request("IFS= read -r signal; printf R; exec /bin/sleep 120")).unwrap();
        let output = process.take_stdout().unwrap();
        assert!(process.take_stdout().is_none());
        let mut input = process.take_stdin().unwrap();
        let (receiver, reader) = parked_reader(output, &mut *input);
        process.close_outputs();
        assert_eq!(
            receiver
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .unwrap(),
            0
        );
        assert!(reader.join().is_ok());
        assert_eq!(process.try_wait().unwrap(), None);
        assert!(process.stop().is_ok());
    }

    #[test]
    fn dropping_the_handle_reaps_a_running_worker() {
        let workspace = Workspace::new();
        let process = spawn(workspace.request("exec /bin/sleep 120")).unwrap();
        let pid = process.id();
        drop(process);
        assert!(
            !Command::new("/bin/kill")
                .args(["-0", &pid.to_string()])
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
