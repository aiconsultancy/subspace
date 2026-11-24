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
  button. Quitting returns the failure exit code.
- It is additive: no existing type or function changes. The Framework names
  no setting; the app writes the words.

## Delivery

- Test: `startup_failure::tests` renders the window and finds its Quit
  button.
- Not verified: the window on Windows and Linux.

## Tasks

- [x] The function, its view and test, and the README note.
