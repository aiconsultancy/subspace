# Report an issue

Report issue files into the GitHub repository the app names. Apps that do
not offer it show none of what this page describes (see
[For app builders](#for-app-builders)).

Use **Report issue** next to About in the toolbar (an icon unless you turn on
its label, or a row in the toolbar menu), or in the status bar. Describe
the problem, then select **Review report** to check the exact title and body.
The final action creates an issue or opens GitHub's issue editor, depending on
authentication and attachments. Opening the browser does not confirm creation.

Use **Steps to reproduce** for the actions that trigger the problem. The field
shows ten text rows and scrolls for longer instructions.

## Check the diagnostics

Expand **Automatic diagnostics** to inspect the compact field list before
review. It starts collapsed and shows the number of included fields.
By default, the diagnostics contain
only the product and app version, build channel and commit, OS and architecture,
registered active board, theme category and icon size. They do not contain paths,
configuration contents, logs, environment values or credentials.
Collapsing the field list does not remove diagnostics from the issue. The exact
preview lists every field that will be sent.

**Include paths** is off for every report. The form does not save or remember
this choice. Turn it on only when path information is needed to diagnose the
problem. The additional fields are:

- The app executable path.
- Whether the app started from a terminal or the desktop.
- Where the app's PATH came from, including an inherited-PATH reason.
- The resolved Azure CLI path, or `not found on the app's PATH`.

The app replaces the home directory with `~`. Other folder and configuration
names can remain visible. The diagnostic fields never include the full PATH,
environment values, tokens, passwords or proxy credentials. Check your own
description and images for private information before sending.

The form warns: "Paths can show folder and config names. Check the preview before sending."
Changing **Include paths** invalidates the previous review. Check the new exact
body before you use the final action. Turning the switch off restores the
original path-free diagnostics.

GitHub's browser URL has a size limit. The preview warns if diagnostics are
omitted from that URL. Use **Copy body** to paste the full reviewed body when
instructed.

## Images and failed submissions

Add up to five PNG or JPEG screenshots. Annotate them before selecting them if
needed. The picker supports multiple selection. Each thumbnail has a **Remove**
action. The count shows the number selected, and **Choose image…** is disabled
at five. Remove an image to make room for another.

Check each image for sensitive hostnames, keys or customer data before sending.
Each image must fit the existing 8 MiB and 8-megapixel limits. The app validates
its content and re-encodes accepted pixels as PNG to discard source metadata.
It does not include filenames or image bytes in the issue body.

The issue-creation request does not upload images. After the report opens in
GitHub, use the numbered **Copy image 1 of 3** sequence. Paste each image into
GitHub before copying the next. If the browser needs the report body pasted,
paste it, then select **I pasted the body** to enable image copying.
**Copy body** disables image copying again until you confirm the paste.
**Copy title** and **Copy body** remain available to recover the reviewed text.

Use **Save images…** to save the validated PNGs in a folder you select, then
attach them in GitHub. The app does not replace existing files. Report images
stay in memory unless you explicitly save them. They are not written to app
state, logs or caches.

The form keeps unsent text if a request fails. If creation is uncertain, follow
the displayed recovery instructions instead of submitting another copy. A
confirmed issue link remains available.

## Copy an error without opening an issue

Select **Copy details** beside an error to copy its message and local debugging
context for GitHub. About also has **Copy diagnostics** without an
error. These local-copy commands include the permitted path facts with `~`
redaction. They do not enable **Include paths** for a future report.

Each listed error also writes one sanitized structured record to the app log
when it occurs. Reopening or rendering the error again does not add a record.
Select **Show log** next to **Copy details** to open the existing log view.
You can also use **View > Show logs** or **Cmd-Shift-L** on macOS. The log view
has **Open log folder** and **Copy log path**. Reporting an issue does not
automatically attach the log.
If Settings has unsaved changes, its existing confirmation appears before
Show log leaves the dialog. Cancel keeps the draft and the current view.

## For app builders

Report issue is behind the facade's `report-issue` feature, which `host`
does not include. Without it, none of Report issue is compiled: the
`subspace-report-issue` crate, the `OpenReportIssue` action, the toolbar
control and its label setting in **Settings > Layout**, the status bar
command and the toolbar menu row.

The Framework names no repository. With the feature, set
`ProductBuild::report_issue_repository` to the repository reports file into,
as `owner/name`; the review step names it ("Destination: owner/name") and the
client creates the issue, its labels and the browser editor link there. A
missing or blank value hides Report issue at run time; a malformed one hides
it and shows a startup notice. Read the value in your build script (as the
Subspace App does with `SUBSPACE_REPORT_ISSUE_REPOSITORY`) rather than in the
Framework, which is not rebuilt when an app's environment changes.
