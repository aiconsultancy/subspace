# A start-up failure people can see

Tracker issue: SUB-60.

## Intent

An app built from the Framework can check its own settings before it calls
`shell::launch`, as the Subspace app checks its API and sign-in settings and
its connection file. Until now a failed check could only print to standard
error and exit, which nobody sees when the app is opened from the Finder,
the Dock or the Start menu: the app simply does not open.

## Spec

- `shell::report_startup_failure(app_name, title, detail) -> ExitCode`
  (`host/app/src/startup_failure.rs`) prints the title and detail to
  standard error, then opens one centred window titled `app_name` with the
  title, the detail and **Quit**, built from the shared dialog footer and
  button. **Quit** or closing the window ends the app. On Linux and Windows
  the event loop then returns and the function returns the failure exit
  code; on macOS quitting ends the process from inside the loop with exit
  code 0, so a script cannot rely on the exit code there and should read
  standard error.
- It is additive: no existing type or function changes. The Framework names
  no setting; the app writes the words.

## Delivery

- Test: `startup_failure::tests` renders the window and finds its Quit
  button.
- Not verified: the window on Windows and Linux, and closing it from the
  title bar on any system (the close hook is not reachable from the
  headless test).

## Tasks

- [x] The function, its view and test, and the README note.
