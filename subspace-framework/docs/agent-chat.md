# Agent chat prototype

Open **Chat** in the toolbar, or press **Cmd+Shift+C** on macOS, **Super+Shift+C**
on Linux, or **Ctrl+Shift+C** on Windows. The right pane takes
space from the dashboard and its inspector. Closing the pane leaves the agent
running. **Chat …** means it is working; **Chat !** means it needs input.

Chat starts **Off** until you choose an engine. Select **Copilot**, **Pi** or
**ACP agent** in **Settings > Chat**, then save and restart Subspace. Pi and
ACP also need their experiment enabled in **Settings > Experiments**.
Copilot uses the official Rust Copilot SDK. The engines keep separate
histories. See [Pi chat](pi-chat.md) for its setup and tool permissions.

The toolbar's **Chat** button, the chat header and the status bar show the
selected engine's mark: the Copilot mark, the Lucide Bot for Pi, or the ACP
agent's own mark (OpenAI for Codex, Claude, OpenCode, Gemini, or the Lucide
Plug for a custom agent). The status bar puts the mark before the engine's
state, such as "Copilot ready" or "Codex ready".

The experimental ACP engine runs OpenCode, Claude Code, Codex, Gemini CLI or
another [Agent Client Protocol](https://agentclientprotocol.com) agent:

- **Presets.** `chat.acp.agent` is `opencode` (default), `claude`, `codex`,
  `gemini` or `custom`; `chat.acp.command`, `args` and `env` override the
  preset's program.
- **Board tools.** Each session gets the running app's own tools, the same
  in-app tools Copilot and Pi chat use, including IRC's when the Channels Space
  runs. The app serves them to that session alone on a loopback MCP endpoint
  with its own token; **Settings > MCP** and its switch do not affect them.
- **Session context.** When a session starts, the agent is told which boards
  are enabled, which tools serve them and that writes need the user's
  confirmation: in its system prompt when it supports one, else once before
  the first message.
- **Approvals.** The agent's permission requests and every board write use
  the chat's approval card, as for Copilot and Pi.

See [ACP agent chat](acp-chat.md) for setup, sign-in and limits.

The Copilot-specific sections below describe the SDK engine. Remote sandboxes
are not part of this prototype.

## Chat tabs

The header holds the engine's mark and name, one tab per open chat, the `+` that opens
another, the Sessions tab and the close button. The chat tabs share the width
between the mark and those controls: one tab takes all of it, adding a tab
shrinks the others, the tab you are in is a little wider, and the strip
scrolls only once the tabs reach their minimum width. A title the tab cuts is shown in full on hover. At most
eight chats are open at once. Each tab
keeps its own transcript, approvals, draft message, context, model, reasoning
effort, agent, mode and access mode. A turn in one tab keeps running while another
tab is shown: a working tab shows `…`, and a tab waiting for an approval or a
question shows `!`.

| Action | How |
| --- | --- |
| New tab | **New session** in the Sessions tab, `/new`, or Cmd/Ctrl+T |
| Close the tab | the tab's **×**, `/close`, or Cmd/Ctrl+W |
| Next or previous tab | Cmd/Ctrl+Shift+] and Cmd/Ctrl+Shift+[ |
| Rename the tab | double-click its title, Enter saves and Escape cancels, or `/rename <name>` |

Closing a tab detaches its session. The chat stays saved and reappears in the
session list. Closing the last tab opens an empty one. A ninth tab is refused with
an error that asks you to close one first.

## Cloud agent replay

**Open in chat** on a cloud task in the Copilot Agents dashboard replays that task's last
session in its own tab. Subspace connects to the task id and resumes what Copilot
reports, with no tools, no permission handler and no workspace: nothing in the
session runs, and the connection changes nothing on the server.

The tab is read-only. The input is disabled, the runtime refuses a message, and a
banner says **Replay of the last cloud agent session; times are from loading**,
because the SDK reports the events at load time rather than at the times the agent
worked. **Open session** beside the banner opens the task's page on GitHub, which
is where the task is steered. A task with several sessions replays only its last
one.

The tab is not saved in `copilot-tabs-<login>.json`, and closing it stops the
transient session. A connect failure is shown in the tab and names the task's
session page and the Copilot Agents log.

Subspace saves the open tabs of each account in `copilot-tabs-<login>.json` beside
`state.json`, and reopens them at the next start with their history. A tab that has
sent no message is not saved. A chat that cannot be resumed is reported once and
dropped from the list; the other tabs still open.

## The Sessions tab

The Sessions tab (the grid icon at the right of the header) replaces the
transcript and the composer with this account's saved local chats. Selecting a
chat tab returns to that conversation; `/sessions` opens the tab from the input.

- **Search sessions** filters by name as you type.
- Chats are grouped by when they last changed: **Today**, **Yesterday**,
  **Older**. Empty groups are hidden. **New session** sits on the first heading.
- Each row shows a badge with the first letter of the name, the name, and how
  long ago it changed. A chat already open in a tab carries a dot on its badge,
  amber while that tab runs a turn.
- Hovering a row adds the created time and the first eight characters of the
  session id. Only values Copilot reports are shown: it does not report a
  message count or a model for a saved chat, so neither is displayed.
- Clicking a row focuses its tab, or opens it in a new one.
- Right-clicking a row offers **Rename**, **Export…** and **Delete…**.
  Renaming edits the name in the row (Enter saves, Escape cancels). Export
  writes the transcript as Markdown to a file you choose; attached references
  are folded to their labels, so no copied service content is written. Delete
  asks first, closes the chat's tab and removes it from this computer.
- Keys in the list: Up and Down move the highlight, Enter opens, F2 renames,
  Delete asks to delete.

## Session names

A chat shows its session name. Without one it shows a title cleaned from its first
message: the attached references removed, one line, cut at 48 characters. After the
first message Subspace offers that title to the SDK as the session's automatic name,
so the Copilot CLI and Subspace agree. A name you set yourself with `/rename` or a
double-click always wins, and the SDK accepts 1 to 100 characters.

## Context and actions

- **Auto context** attaches the current dashboard selection with a message.
- **+ Current** or `/add-context` attaches a selection manually.
- **Add to chat** pins the selected row with everything its detail
  panel shows: the properties, the body or description, the comments,
  reviews, evidence and code the panel renders, as labelled text. It is on
  `Cmd/Ctrl+Shift+A`, the detail panel's title bar, and the palette. The chat
  pane opens if it was closed. One reference holds at most 16 KB and says
  where it was cut.
- A right click opens the same menu on any row, on a group header that stands
  for an item, and anywhere in the detail panel. It selects the line first, so
  the entries act on the line that was clicked. With text selected in the
  panel it also offers **Copy** and **Add selection to chat**; that
  reference names the row it came from.
- **Attach file** adds an explicitly selected file to the local workspace.
- Remove a reference with its pill's **×** control. Removing the automatic
  selection also turns automatic context off.
- **Tools** and **Thinking** control visibility of SDK activity. Expand an entry
  to inspect it. The UI cannot expose thinking that the runtime does not send.
  A tool row names the call once: when Copilot's description of a call is the
  argument it passes, the argument is shown and the description is not
  repeated. A call the turn ended before finishing is marked "did not
  finish".
- **Stop** requests cancellation. It cannot undo a service write already sent.
- The input stays live while a turn runs. A message you send then steers that
  turn: it goes with the SDK's immediate delivery, so Copilot takes it into the
  work in flight instead of queueing it for the next turn. The transcript marks
  such a message **Steering** above its bubble, from the delivery the SDK
  reports. Attached references go with it, because they are part of the message
  text. A slash command, a rename and the access and model controls still wait
  for the turn to end: Copilot applies those to the next turn, not the one in
  flight. **Stop** is unchanged.
- While a turn runs the panel says what Copilot is up to, in a phrase that
  changes every few seconds. It is decoration: the Stop control and
  everything else behave the same whatever it says.
- A message typed before Copilot has finished starting is held and sent as
  soon as the chat is ready, so nothing typed at launch is lost.

## Canvases

Copilot can open a native **Chart**, **Diagram** or **Drawing** beside the
conversation. The canvas area sits between the board and the chat pane and
belongs to the chat tab: each tab has its own list of up to 16 open canvases,
and the area follows the tab you switch to. Canvases are a Copilot feature;
Pi chats report that native canvases require Copilot.

- **Chart** takes Flint `ChartAssemblyInput` with inline data rows. It is
  compiled by the embedded QuickJS compiler and drawn by the shared chart
  widget. **Data** at the foot
  of the area shows the values as a table.
- **Diagram** takes Mermaid source (flowchart, sequence, class, state, ER and
  gantt subsets; at most 200 lines and 16 KiB). An unsupported diagram shows
  the error and the source. **Source** shows the Mermaid text.
- **Drawing** takes a restricted SVG subset with explicit width and height:
  basic geometry and text, no scripts, images, `foreignObject`, links or
  external resources. Everything else is rejected before rendering.

The header shows the kind glyph, the title, the position (`2 of 3`),
previous and next, **Zoom**, close (**×**) and hide. The caption names the
source and the time the canvas opened. Drag the left edge to resize; the
canvas takes width from the board, and board, canvas and chat keep usable
minimum widths. In the dock a chart keeps a capped aspect and a diagram or
drawing keeps its own proportions up to a square, at the top of the column;
Zoom shows either at full size.

**Zoom** replaces the board with a gallery: the canvas fills the space, the
chat stays open, side arrows and a filmstrip step through the list, and the
toolbar shows **Canvas gallery** with the board tabs dimmed. `Esc` closes the
gallery and returns focus to the chat input. `←` and `→` step while the
canvas has focus; they never take arrow keys from the input or other text
fields. `Cmd/Ctrl+Alt+G` opens and closes the gallery from the chat without
colliding with the boards' plain `g` keys or the chat tab keys.

Every `open_canvas` call leaves a card in the transcript: the kind, the
title and **Show in canvas**. The card of the canvas on show is highlighted.
Close (**×**) removes a canvas from the list; its card reopens it through
Copilot. Hide collapses the whole area and the chat header shows
**Canvas (n)** to bring it back.

Canvas input lives in the SDK session, not in a Subspace cache. Resuming a
chat recompiles the persisted inputs, so the open canvases and their cards
come back after a restart; a canvas that no longer compiles shows a visible
restore error. Canvas input that looks like a credential is rejected. The
canvases declare no custom actions, so Copilot can only open and close them,
and they are not offered by `subspace mcp`. The Copilot SDK is pinned to
`1.0.34`; when the service rejects canvas negotiation the chat starts again
without canvases and says so.

## Notifications

**Settings > Notifications** has separate controls for completed turns,
requests that need input, OS notifications and sound. Completion and input
alerts are on by default. OS banners and sound are off by default. The choices
apply when you select **Save**; **Cancel** leaves the saved choices unchanged.

Alerts use generic status text. They do not include conversation titles,
prompts, replies, file paths or other chat content. In-app alerts still appear
when OS delivery is disabled, unavailable or denied. OS banners are best effort:
macOS requires a bundled app, and the operating system can suppress delivery.
Sound is independent of OS banners. Linux sound requires an installed
`canberra-gtk-play`; unsupported systems remain silent.

Copilot can query configured services and repositories. It asks for clarification
when needed. Approved service write batches state the exact targets and content.
An uncertain write is not retried automatically; inspect the target first.

## What chat can write

One approval covers one batch of 1 to 20 operations of one board's write tool.
Denying it writes nothing. Every operation is validated and its target read
before you are asked, with the same rules the dashboards use, and only in the
configured repositories. A batch cannot mix boards: changes to GitHub and Copilot Agents
are two calls with two approvals.

| Service | Chat can write |
| --- | --- |
| GitHub | comment on an issue or pull request; change a pull request's title, body, base branch or state; approve, request changes or comment as a review; add or remove assignees |
| Copilot Agents | start a Copilot cloud agent task |
| Channels | send a message or `/me` action to a joined channel or a nick, join or leave a channel, connect or disconnect a configured network, at most 10 per call, as you through the board's live connections |

A Copilot cloud agent task is not a local edit: it starts an autonomous agent
that writes code in the repository on its own and opens a pull request, and it
keeps running after the chat. The approval preview says so first, then names the
repository, the base branch it resolved and the full prompt. The task starts on
exactly that branch, and the result holds the task link and the pull request when
there is one. Only the configured repositories are in reach.

Chat never merges, deletes, changes credentials or makes arbitrary HTTP calls.

## Tools

Chat has the agent tools of the boards the product selects, `ask_user`, and the
Copilot CLI's own tools. Subspace registers `github_read`, `github_write`,
`github_copilot_agents_read`, `github_copilot_agents_write`, `irc_read` and
`irc_write`, plus the shared `board_help` and `web_read` tools and, when a searchable
board's index is available, `search_content`, `read_search_result` and
`related_items`. A
board the config disables contributes no tools. MCP exposes matching read-only
resources for board help, retained search and cached documents; it never
exposes a write as a resource. The complete action
list per tool is the table in [the MCP server](mcp.md), which a test keeps in
step with the compiled catalog. Other harnesses reach the same tools through
MCP, where writes run without a Subspace approval card. Subspace asks the CLI which built-in tools it has
when chat starts, so a CLI update needs no Subspace change; if that question fails,
the list pinned for CLI 1.0.90 is used:

| Group | Tools |
| --- | --- |
| Shell | `bash`, `read_bash`, `stop_bash`, `list_bash` |
| Files | `view`, `glob`, `grep`, `create`, `edit` |
| Web | `web_fetch` |
| Sub-agents | `task`, `read_agent`, `list_agents`, `write_agent` |
| Skills | `skill` |

Tools that drive the CLI's own terminal interface are left out: Subspace draws the
chat, so they have nothing to act on.

### What asks first

| Request | What happens |
| --- | --- |
| Read a file inside Subspace's private workspace | runs, no card |
| Read any other path | approval card naming the path |
| Shell command | approval card with the full command |
| Create or edit a file | approval card with the path and the diff, or the size of a new file |
| Fetch a URL | approval card with the URL, and the page it was redirected from |
| Sub-agent task | approval card with the agent's prompt |
| MCP or custom tool | approval card with the server, the tool and its arguments |
| A path that looks like a credential store | refused, no card |
| A request to leave the sandbox, or to widen it | refused, no card |

`view`, `glob` and `grep` run in the CLI's own process rather than in the sandbox,
so only a read that resolves inside the workspace is silent. The path is resolved
first, so a symbolic link pointing out of the workspace still shows a card. A card
appears whenever enterprise policy requires a human decision, whatever the path.
Long diffs and prompts are shortened in the card and say how much was left out.
Tokens are removed from every card.

**Approve all** and YOLO answer these cards for you; nothing else changes which
requests are made.

## Execution mode

Default mode uses Copilot local sandboxing and requires write approval. Unsupported
isolation blocks execution. App-defined service tools run in Subspace and enforce
their own scope and approval checks.

Inside the sandbox, `gh` and `git` work as the chat's GitHub account without seeing
its token: the child process gets a fake credential and the CLI's local proxy puts
the real one back only for github.com, api.github.com and uploads.github.com. For
`gh`, a repository's own GitHub account wins over the Copilot login; Subspace's
workspace holds no repository, so the chat's account is used. For `git` over HTTPS,
github.com uses the Copilot token and any other forge uses the credential helper on
this computer.

The sandbox lets shell commands reach the network: outbound traffic is on by
default and Subspace adds no host allow-list, so a command can reach any host the
computer can. Subspace does not widen that policy, and it has not narrowed it
either. Treat a shell command as able to reach the internet.

`/yolo on` and the mode control show a warning before requesting full host access
and ordinary approval bypass. The shell stays available in YOLO, without the
sandbox: commands run on the host, and the masked `gh` and `git` credentials are
not applied, because masking belongs to the sandbox. Enterprise policy can refuse
the request. `/yolo off` restores the default policy. Subspace does not restore YOLO
after a restart.

## Authentication and extensions

Subspace chat requires Copilot CLI/SDK authorization, separate from VS Code
Copilot. Working Copilot chat in VS Code does not establish CLI/SDK access.
If your organization blocks Copilot CLI, or you receive
`not authorized to use this copilot feature`, contact your administrator to
request CLI/SDK access. **Reconnect** retries chat startup after access is
approved; it does not bypass organization policy, identity verification or
sandbox requirements.

The error panel shows this authorization guidance for local Copilot chats.
It does not mean Subspace has detected a policy block. A model-list failure
can have other causes; the error's SDK category and RPC code, when available,
help distinguish them.

Chat uses the same account as the GitHub (pull request) dashboard. Choose it
in **Settings > GitHub > Account**; the list shows the github.com accounts signed
in through Subspace and those that `gh auth status` shows. Subspace uses the
account's sign-in when it has one, else its token from
`gh auth token --user <login>`, when chat starts. `GH_TOKEN` and `GITHUB_TOKEN`
do not change the chat account.

When chat has no account, or GitHub rejects a signed-in token, the error offers
**Sign in to GitHub**, which runs Sign in with GitHub (`subspace-github-auth`)
in the app and reconnects when it finishes. When GitHub rejects the token of
`gh`'s active account, the same button runs `gh auth refresh` instead; for
another `gh` account it signs that login in to Subspace. An app without a
GitHub OAuth App (`ProductBuild::github_oauth`) offers no Sign in to GitHub:
the error names `gh auth login` and, for `gh`'s active account, `gh auth
refresh`. A changed account takes effect after a restart. Each
account keeps its own saved conversation. A conversation is saved when its first
message is sent; New chat clears the saved one, so a restart before that message
starts an empty chat.

Copilot must report the chosen login; a mismatch blocks chat rather than selecting
another account. Never put a token in a chat message or attach a credentials file.

To override the account for chat only, use a private `copilot.json` beside the
platform Subspace configuration, with permissions `600` on macOS/Linux:

```json
{
  "login": "your-work-login_company",
  "enableMcp": false,
  "enableSkills": true
}
```

The prototype's explicit `token` setting accepts fine-grained `github_pat_` PATs
only. The SDK also supports OAuth/App user tokens, but the private-file validator
does not yet accept those token types. Classic `ghp_` PATs are unsupported. Copilot
access and enterprise policy still apply.

The default Subspace agent is bundled as `subspace.agent.md` and cannot be replaced
by a custom agent with the same name. Add custom `*.agent.md` files to
`~/.config/subspace/copilot/agents/`, then restart Subspace. This is the only default
agent directory; `~/.copilot/agents` and repository agent directories are not
imported. An explicit `agentDirectories` list in the private `copilot.json`
overrides the default directory; an empty list keeps only the bundled definition.

Custom files use Markdown with optional YAML front matter, for example:

```markdown
---
name: reviewer
description: Review work evidence and report concrete findings.
---

Read the supplied evidence. State findings with source links and uncertainty.
```

The SDK parses custom-agent metadata. Subspace reads the authored prompt from its
validated private staging copy, because discovery can omit prompt text. Custom
agents retain the configured tool and skill restrictions. Embedded MCP server
declarations are blocked. New and resumed chats keep automatic configuration
discovery disabled and custom-agent execution local.

Skills load from `~/.copilot/skills`, `~/.agents/skills` and any `skillDirectories`
in the private `copilot.json`. Set `"enableSkills": false` to switch them off.
Instruction files load from the Copilot user location `~/.copilot` through the
SDK session option `instructionDirectories`, with `skipCustomInstructions` set to
false. Repository instruction discovery stays off: Copilot's working directory is
Subspace's private workspace, `enableConfigDiscovery` is false and
`enableOnDemandInstructionDiscovery` is false, so no repository is searched.

Each account gets its own Copilot configuration directory,
`copilot-workspace/runtime-config/<login>`, so one account never sees another
account's saved chats.

**Current limitation:** enabling MCP in chat blocks startup. This is about
chat using external MCP servers, not about Subspace serving its own tools with
`subspace mcp`. The pinned SDK cannot
establish verified isolation before its high-level session-creation hooks run.
MCP configuration is parsed, but MCP servers are not started until that boundary
can be enforced. Do not treat this as working MCP support. Do not inherit
integrations merely because another Copilot installation has them.

Quota and usage labels follow the SDK's units. Request quota is not an AI-credit
balance, and unavailable data is not displayed as zero.

## When a turn fails

When Copilot cannot finish a turn, the chat shows a title for the kind of
failure and one line that says what to do:

| Title | When | What to do |
| --- | --- | --- |
| Copilot returned no reply | The runtime accepted the message, but the turn ended with no visible output (`errorType` `query` with no HTTP status) | Send the message again. If it happens again, pick another model or a lower reasoning effort. |
| Copilot did not accept the sign-in | `authentication` or `authorization`, a sign-in or account remediation, or HTTP 401 or 403 | Sign in to GitHub again, or use an account with Copilot access. |
| Copilot rate limit or quota reached | `rate_limit` or `quota`, or HTTP 402 or 429 | Wait for the limit to reset or pick another model. |
| A Copilot policy blocked this request | `policy` | Pick another model, or ask your Copilot administrator for access. |
| The conversation is too long for the model | `context_limit` | Start a new chat, or pick a model with a larger context. |
| Copilot could not reach its service | HTTP 407, 502, 503 or 504 | Check the network and proxy settings. |
| The Copilot runtime stopped | The runtime process exited, so its event stream closed or a message could not be delivered | **Reconnect**, then retry. |
| The Copilot runtime is not responding | A turn had no event for 90 seconds and the runtime did not answer a ping within 15 seconds | **Reconnect**, then retry. |
| Copilot could not finish this turn | Any other failure | Retry. If it happens again, copy the details into a report. |

**Details** shows the raw error type and message, the HTTP status, the error
code, the GitHub and Copilot service request IDs, the message ID, the model and
reasoning effort, the runtime and SDK versions, the time, how long the turn
ran, whether a reply had started, and the number of attachments. Fields that
the runtime did not send are left out. **Copy details** copies the same lines.
**Retry** sends the message that started the failed turn again, with its
context and images, so you do not need to type it again. A message sent while
the turn was running does not replace it.

Each failure is also written to the app log as one `agent` record at error
level, `turn failed`, with the same fields. Debug level adds `turn started` and
`turn done` records with the model, effort, attachment count, duration and
outcome (`done`, `failed`, `stopped` or `not_responding`). Subspace never adds
prompt text, attachment contents or tokens to a record or to Details. Every
field the runtime sends is redacted for credentials, and any copy of the turn's
own message or context text of 4 or more characters is replaced with
"[message text removed]".

If a turn that was reported as not responding produces output later, the
runtime has recovered: the chat shows the output and the turn continues.

## Platforms and verification

macOS is the initial runtime target. Copilot local sandboxing uses Seatbelt on
macOS, bubblewrap on Linux, and ProcessContainer on supported Windows builds.
Linux needs the runtime's documented bubblewrap/network prerequisites. Unsupported
hosts or policies must not fall back silently to unrestricted execution.
