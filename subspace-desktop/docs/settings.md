# Settings and appearance

Open **Settings** with the cog between the eye icon and **?**, from the
command palette, or with **Cmd+,** on macOS (**Ctrl+,** elsewhere).
The eye icon opens the existing density controls; the backtick shortcut
still works.

## Sections

The sidebar groups application settings separately from Spaces.
General explains storage and credential priority. Each dashboard section holds
its connection settings and token input. Save and Cancel stay in the footer;
overflowing content has a vertical scrollbar.

Debug builds add **Developer** at the end of the application pages. Its
**Developer mode** switch, on by default, shows the Developer menu, the
inspector and their palette commands; Save applies it without a restart. See
[Developer tools](../../subspace-framework/docs/developer-tools.md).

## Notifications

**Settings > Application > Notifications** starts with **Play notification
sounds**, on by default. Turn it off and choose **Save** to mute Copilot and
every Space, including OS banner sounds. Each Space's Sound choice is kept
for when sounds are on again. Cancel discards the edit. The app preference
is `notifications.sounds` in `state.json` (`1` on, `0` off).

The **Copilot** section applies only to Copilot chat: turn completion,
waiting for input, OS notifications and sound. Space rows keep their own
delivery, style and Sound choices. **Preview sound** deliberately plays even
when the master is off. **Test notification** follows the saved master
choice; neither button saves the draft. OS permission, Focus and retention
remain controlled by the operating system.

## Updates

When the app has an update source, **Settings > General > Updates** has
**Automatically check for updates at launch**. It is on by default. The choice is per
app. Save applies it. The app stores it in that app's `state.json`
preferences as `app.check-updates`.

When the setting is on, the app checks once after the window opens. A newer
release shows a toast with the version. A failed check shows no toast.
The About dialog shows the check status, including when the selected channel
has no published release and when GitHub declines access. The updater uses
the selected GitHub account after any explicit update token variables. If the
selected account has no token, About asks you to sign in with that account.

The About dialog has **Check for updates**. When a newer release exists,
it also has **Install**. The same launch-check option is available in About;
changes there apply immediately and use the normal preference save cycle.
Checks do not install updates or restart the app automatically. Installation
shows download progress and verification stages. See
[Update an installed app](installation.md#update-an-installed-app).

## Services

The dashboard sections edit GitHub and other board service settings in the
existing platform config file:

- macOS: `~/Library/Application Support/subspace/config.json`
- Linux: `~/.config/subspace/config.json` (or `$XDG_CONFIG_HOME/subspace`)
- Windows: `%APPDATA%\subspace\config.json`

**Settings > General** shows the path of this file. **Show in Finder**
(**Show in File Explorer** on Windows, **Show in Folder** on Linux) beside it
selects the file in the system file manager, or opens its folder when the file
does not exist yet.

A board can own its settings controls (`Dashboard::settings_editor`). Its page
shows them below **Enabled**, inside the same Save and Cancel draft; Save
validates them with the rest of the dialog and writes only the keys the board
owns to the board's own settings file, `<id>/config.json` in the folder of the
config file (for IRC, `~/.config/subspace/irc/config.json` beside
`~/.config/subspace/config.json`). The file's top-level object is the board's
settings; other keys in it are kept. Like the config file, it is written
owner-only (`0600`) in an owner-only folder (`0700`), and Save refuses to
overwrite it when it changed since Settings opened. A board file that other
users can read or write is ignored at startup with a notice. Only these native
board settings have their own files: web Spaces (`webBoards`), board choices
and service settings stay in the config file. **Settings > Channels** is one: it
edits IRC networks and highlight words, offers **Copy agent prompt** and
**Show in Finder** for the IRC file, and applies saved networks without a
restart. **Notify ship captains** is on by default and applies on Save. It
posts PR merge and deployment updates to matching initiative channels on
the initiative's IRC connection while automatic board refresh is enabled. See [Channels](../spaces/irc/help.md#notify-ship-captains).

In a build with the terminal, **Settings > Terminal** (under Application) sets
the terminal's font family and size for open and new sessions on Save, as
`fontFamily` and `fontSize` in the config file's `terminal` object. See
[Terminal font configuration](terminal.md#configuration).

**Settings > Layout > Space order** lists every Space. Drag its row or use
the move buttons, then choose **Save**. You can also drag visible tabs across
the top of the app. A tab drop saves the new order immediately, and the
active Space stays open. Settings can also reorder Spaces that are hidden
or in the overflow menu. The saved order controls the navigation rail,
number and function key shortcuts, and previous/next navigation.

The order is stored as `boardOrder` alongside `autoRefresh` in the same
config file.

To close a Space without opening Settings, right-click its tab (or its
destination in the navigation rail) and choose **Close Space**. This saves
and applies the same choice as turning off the Space's **Enabled** switch and
choosing **Save**: the tab goes at once and the config file's `boards` entry
matches what Settings writes. Closing the current Space opens the tab before
it, or the next tab when it was first. The last open Space can't be closed;
its menu says so. Nor can the last Space the config file leaves on, if the
file was edited outside the app. A notice says where to add it again: the
plus in the title bar. **Space settings…** in the same menu opens that
Space's Settings page.

To turn a Space back on without opening Settings, choose the plus after the
last tab (**Add a Space**; in the navigation rail it is the last item). Its
menu lists every Space that is switched off, with its description. Choosing
one saves and applies the same choice as turning on its **Enabled** switch
and choosing **Save**, then opens it. A notice confirms it ("Opened
Actions."), or says that a restart is needed or why the choice could not be
saved. When every Space is on, the menu says so. **Manage Spaces…** at the
end of the menu opens Settings at the first Space that is off, or at the
first Space when all are on. When the tabs run out of room, the plus moves
into the chevron menu as **Add a Space…**. The command palette has the same
**Add a Space…** command.

Automatic board loading and refresh is on by default. Subspace warms boards
after startup and refreshes them at staggered intervals around 60 seconds.
Saving this switch applies it immediately: turning it off prevents new
background loads (a load already in flight can finish), and turning it on
resumes at the next board deadline without a restart.
Only a board's safe data-only hook runs: it does not activate a view or
request interactive authentication. Boards keep the last successful
data when a refresh fails and do not start another load while one is in flight.

In a build with the terminal, **Settings > Layout** also has **Terminal
placement** (Left, Bottom or Right, default Bottom), **Bottom terminal width**
(Beside the chat, the default, or Full window width) and **Show text on
terminal layout buttons** (off by default). They are stored in the state file
as `terminal.placement`, `terminal.bottom-span` and `terminal.button-labels`
and apply on Save. The buttons in the terminal's header change the same
choices at once. See [Terminal layout](terminal.md#layout).

**Settings > Layout** also controls the application toolbar at the top right:
Chat, Terminal (in a build with the terminal), Search, Commands, Reload,
Wrap, the theme designer, Settings, Help,
About and Report issue (in a build with Report issue; see
[Build configuration](installation.md#build-configuration)). **Application toolbar labels** has one switch per
control; every control starts icon-only, with its name as tooltip and
accessible label. Right-click a toolbar control and choose **Show label** to
toggle its label without opening Settings. **Collapse application toolbar into
a menu** replaces those controls with one button with a burger glyph,
**Toolbar menu**. Its menu lists every control by icon and name in toolbar
order, with each shortcut, and checks the ones that are on, such as Wrap or
an open Help panel. While the toolbar is collapsed the label switches are
disabled because the menu always shows the names. Both choices are app
preferences (`app.toolbar-label.<control>` and `app.toolbar-menu`) and apply
on Save. The status bar, including its **Report issue** item, is not affected.

Token inputs are masked. Saved tokens are not copied into the inputs. Leave
a token field blank to keep its saved value. Use **Clear saved token** to
remove it on Save; **Undo clear** cancels that removal. Board password fields,
such as IRC's, use the same control with **Password saved** and **Clear saved
password**.

Environment variables still take priority over config tokens. GitHub then
uses the chosen account: its sign-in through Subspace, else its CLI
credentials. Removing a token from this file does not remove an environment
variable, a sign-in or a CLI login.

### Sign in with GitHub

Every GitHub Space's page (PRs, Actions, Copilot Agents) starts with
**GitHub accounts**: each account with where its token comes from (**Signed
in**, **Sign-in expired** or **GitHub CLI**), **Sign out** for signed-in
accounts, and **Sign in with GitHub**. Sign-in and sign-out apply at once;
they are not part of the Save draft. The **Account** select below chooses
the account the Spaces and chat use, and that choice still waits for Save.

- A release build opens GitHub in the browser. Approve Subspace there, then
  GitHub sends the browser back to Subspace, which shows the new account and
  comes to the front. While it waits, the page says **Continue in your
  browser** with **Cancel**. The sign-in gives up after five minutes.
- A build without the OAuth App's client secret (development and capture
  builds) shows a code with **Copy code**, **Open GitHub** and **Cancel**
  instead. Enter the code on GitHub's device page. GitHub must allow device
  sign-in for the app; when it does not, the page says so.
- GitHub may ask which account to use when you are signed in to several.
  Subspace asks for the `repo` and `read:org` scopes and refuses a sign-in
  that lacks either.
- **Sign out** forgets the account's token and asks GitHub to revoke the
  app's grant. A `gh` account of the same login is not changed.
- A build without an OAuth App (`SUBSPACE_GITHUB_OAUTH_CLIENT_ID` set empty
  at build time; see
  [Build configuration](installation.md#build-configuration)) offers no
  **Sign in with GitHub** and no **Sign out**; the page lists the `gh`
  accounts and its notes name only `gh auth login`.

Signed-in tokens go to the system keychain (debug builds: an owner-only file
beside `state.json`), never to `config.json`, `state.json` or a Settings
input. When GitHub rejects a signed-in token, the account shows **Sign-in
expired** until it signs in again; requests fall back to `gh`'s account of
the same login if there is one. Chat offers the same sign-in when it has no
working account. See [Settings and stored state](../README.md#settings-and-stored-state).

**Favourite repositories** lists the GitHub repositories (`repos`) that the
PRs board's **All open**, Copilot Agents, Actions and the agent tools use.
Each row has **Remove**; **Add repository…** opens the shared repository search
(your recently pushed repositories, or GitHub's search as you type), where a
pick stars or unstars a repository. These edits stay in the draft and apply on
**Save** without a restart. The PRs board's own **Repositories** control edits
the same list but saves each star at once.

If `gh auth status` succeeds in a terminal but the app cannot obtain a token
and you prefer `gh` to Sign in with GitHub,
check whether the app can find `gh`. Finder-launched apps can inherit only
system executable paths, while Homebrew installs `gh` in `/opt/homebrew/bin`.
The app already calls `gh auth token` as its fallback. Quit the existing app
and use `make run` from the authenticated terminal as a temporary workaround;
another `gh auth login` does not correct a missing executable path.

Save validates the settings and replaces the config file atomically. On
Unix, the resulting file is readable and writable only by its owner
(`0600`). The dialog preserves unrelated JSON properties and refuses to
overwrite malformed JSON or changes made externally since it was opened.
Reopen the dialog after an external edit.

**Restart Subspace to apply saved service or credential changes.**
Automatic refresh, the terminal's layout and fonts and board Enabled choices
apply on Save. **Close Space** in a tab's right-click menu and the
**Add a Space** plus in the title bar save and apply one Space's Enabled
choice at once.
Cancel, Escape, or the
close button discards unsaved edits. Existing service tokens are stored as text
in the private config file, not in Keychain, UI state, or theme files.

### Subspace API connection

Issues uses one built-in **Subspace API** profile (`subspace-api`) at the
API URL the build config or `SUBSPACE_API_URL` names (by default the local
stack, `http://127.0.0.1:3000`). Requests carry the bearer token of the
account in **Settings > Account**, from the identity provider the build
selects; see
[Identity providers and the API URL](../README.md#identity-providers-and-the-api-url).
There is no organisation or profile picker or saved issue profile
configuration. `GET /v1/me` checks the connection and identifies the user.

Debug builds can pair a numeric-loopback `SUBSPACE_API_URL` with
`SUBSPACE_API_ACCESS_JWT_FILE` (a private file holding a bearer token) for
offline fixtures. Release builds ignore the file. Never put the JWT in Settings, state or logs. See
[Issues and Subspace API](issues.md#debug-overrides-and-offline-fixtures)
for the isolated fixture and commands. Other services retain their existing
credential priority and storage.

## Appearance

The Appearance section selects an available GPUI Kit theme and installed
fonts for headings, body text and code. These choices preview immediately.
Save persists them for startup;
Cancel, Escape, and the close button restore the previous appearance.

The GPUI Kit theme collection is bundled with the app and available offline.

**Space navigation** selects horizontal tabs or a vertical rail on the left.
**Use icon-only Space navigation** collapses the vertical rail to icons.
Save applies and remembers both choices without a restart. Cancel leaves
navigation unchanged. The command palette's **Toggle vertical navigation**
command and the rail's collapse button remain available.

**Cupertino Dark** is the shared default unless a build or product chooses
another theme. Available saved user choices still win.
**Automatic (Subspace default)** uses the original font fallback stack:
JetBrains Mono, SF Mono, Menlo, Consolas, then common Linux monospace fonts
and the system font. Density settings remain separate and unchanged.

To add a theme, place a GPUI Kit theme JSON file in the `themes/` directory
beside `config.json`, then restart Subspace. The file uses GPUI Kit's
`ThemeSet` format (a `themes` array), as in
`subspace-framework/host/app/themes/one-dark.json`. Themes cannot replace bundled names,
including **Subspace One Dark**.
Invalid theme files are reported without
preventing startup.

If a selected theme or font is removed, Subspace uses its default instead.
Dashboard colours follow the selected theme. Colours supplied by a
service, such as GitHub label colours, keep their service-defined values.

Service settings and appearance preferences have separate file-save
boundaries. If service settings were saved but appearance could not be
saved, the dialog reports that partial result and stays open for a retry.
