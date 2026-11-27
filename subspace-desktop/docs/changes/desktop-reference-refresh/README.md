# Decision and pull request reference refresh

## Intent and specification

Keep Channels references stable while their summaries refresh. A GitHub
Reload revalidates the references immediately and keeps their visible
summaries until newer answers arrive. An account change clears those summaries
and an access refusal removes the affected details.

A project directory error can leave a person signed in to Decisions. A
forbidden project directory clears the project names. Decisions scoped to an
initiative apply the list limit to that initiative's decisions. Channels
enables decision chips after the connected API returns a decision identifier.

## Delivery

- Reload marks cached PR details unconfirmed and notifies Channels to
  request updated summaries. Opening an unconfirmed detail rechecks access.
- Channels shows PRs as loading while GitHub discovers its account. The
  inline PR glyph has a muted colour until the first summary lands. Ship inspector rows keep the same identity as their summaries
  change. Hidden issues can be rechecked from the Ship tab.
- Subspace reads each project in an initiative and merges results in API
  cursor order. NETT reads until enough matching results arrive or its cursor
  ends because its Decisions endpoint has no project filter.
- The interface fixture checks the released Space order and the Copilot
  Agents command only when it is compiled in.

## Capture evidence

The macOS fixture used Twilight Two, a loopback IRC ship channel, a slow
GitHub detail read and an API that refused the decision directory request.
The before build used `74b4e312` from main.

- [Startup before](evidence/channels-startup-before.png) shows a premature
  signed-out PR tile and unavailable rows in **In play here**.
- [Startup after](evidence/channels-startup-after.png) shows the resolved
  summaries and the decision reference as plain text.
- [During Reload](evidence/channels-refresh-after.png) shows Channels after
  switching to PRs, refreshing, and returning while detail requests run.
- A separate run used an API with decision identifiers. The reference shows
  the title and status on [hover](evidence/decision-hover.png), then opens
  the [decision and its Back control](evidence/decision-opened.png).

## Board checklist

The existing Space IDs, navigation contract, keyboard actions, storage and
components are reused. Refresh follows the account that issued the read and
retains the last answer while revalidating. The existing conversation return
location is carried by Decision chip activation.

## Tasks

- [x] Correct default product interface checks.
- [x] Cover multi-project initiatives and project-directory failures.
- [x] Cover Decision chip activation, hover, and its conversation return point.
- [x] Cover PR Reload, access loss and a Channels refresh with a visible summary.
