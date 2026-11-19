# Issue workflow end states

The issue page draws the NETT team's workflow with one end slot. The slot
shows Done while an issue is active and shows its current end state after it
reaches one. Editable issues can choose a completed, cancelled or duplicate
state from the shared picker. Trackers and read-only issues show the path
without a picker. Duplicate sets the state using NETT's existing issue edit.

The picker sends a versioned state update through the NETT client. It stays
busy until a read at the written version or later confirms the change. The
write refreshes the issue if it is open again when the response arrives.

Tests cover workflows with custom state names, terminal-state ordering,
trackers, and reopening an issue while reads still return a version older
than the saved write. The macOS fixture captures show a real picker interaction and the issue
reloaded as Cancelled.

| Before | End slot | Picker | Saved state |
| --- | --- | --- | --- |
| Baseline | Done | Picker | Cancelled |
