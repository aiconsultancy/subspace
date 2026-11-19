# Restart after an update

## Intent

After installing an update, the restart button in About must restart the app or show the quit confirmation when enabled.

## Spec

- About closes its dialog and requests a restart from that window's shell.
- Settings uses its existing Save, Discard and Cancel flow before requesting a restart.
- The shell uses the saved quit preference and runs the existing restart path, including preference persistence and running-program warnings.

## Delivery

The update button used to dispatch an action from the focused element. The About dialog's focus path does not include the shell listener. About now calls its own shell directly. Settings resolves its draft through its dialog lifecycle before calling the same shell command.

The regression tests click the installed update button in the actual About and Settings dialogs. They check the platform restart request, the optional quit confirmation and the Settings cancellation path.

## Tasks

- [x] Route both update buttons to the owning shell.
- [x] Add GPUI click regressions for restart and cancellation.
