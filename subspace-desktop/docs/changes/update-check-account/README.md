# Update checks use the selected GitHub account

The About updater uses the selected GitHub account for checks and installs.
Explicit token variables still take priority. The updater tries the selected
Subspace sign-in, then that account in gh. If both are missing, About asks
you to sign in as the selected account. With no selection, the updater uses
the usual login resolution. The command-line updater keeps its existing account order.

A successful check with no compatible release shows a quiet no-release status.
Permission, network and repository errors continue to show diagnostic actions.

The updater tests cover account precedence, the no-release outcome, and
actual HTTP failures. The macOS captures use the loopback GitHub fixture with
an empty release list. The baseline updater was pointed at the fixture for
the capture.

[Before](evidence/before.png) · [No release](evidence/after.png) · [Missing selected account](evidence/missing-account.png)
