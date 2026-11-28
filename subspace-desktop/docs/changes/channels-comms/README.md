# Channels threads and Comms

## Intent

Keep follow-ups in their IRC threads, reduce repeated fleet updates, and give
people one place to catch up on mentions and replies.

## Behaviour

- Fleet channels show a thread's original message and reply count. An older
  thread has one entry point while its original is outside loaded history.
  The inspector can show earlier replies and copy a link to its message.
- The fleet navigator shows the current audit and ship channels once.
  **Other** starts collapsed and can be opened using the list's existing
  disclosure control.
- **Comms** summarises mentions and replies to threads the user participated
  in. Its badge counts unread activity and marks unread mentions with **@**.
  The **All**, **Mentions**, **Replies**, and **Unread only** filters change
  the visible summaries. Selecting a row opens the right-hand Thread
  inspector and marks that summary read for the session. Other messages
  in the channel retain their unread state.
- Captains and the rear admiral use original threads, concise updates and
  shareable thread links.

## Delivery checks

- The existing Channels Space reuses the workspace navigator, toolbar and
  inspector. The Comms pane uses a virtual list with fixed-height rows.
  Its selection is workspace state and the process hub owns unread activity.
- [Channels Help](../../../spaces/irc/help.md) covers Comms and thread links.
  The IRC transport and the existing reply composer send the replies.
- Fixture tests exercise thread-only channel rows, history fallback,
  duplicate navigator entries, Comms selection and independent unread counts.

## Board checklist

- **Layout and components.** Comms opens from the existing Channels Workspace
  navigator. The pane uses the same text edge, toolbar strip and shared UI
  components. Its fixed-height list uses GPUI Kit. The existing inspector and
  shell dock switches show the selected thread. Filters and read actions are
  available in the pane. The inbox has an empty state and an expired thread link
  reports its outcome.
- **Data and attention.** The process hub derives summaries from held or restored
  IRC messages. The existing history and channel read markers remain the source
  of unread activity. Comms keeps its own local read state for the app session.
  The Comms badge shows the unread count and mentions. Existing system
  notifications continue to use the Channels process hub.
- **Links and search.** In-message links use the Channels reference handler and
  the source IRC network. Links to unloaded messages join the channel if
  needed before asking that server for permitted history. Search results for a hidden reply open its thread.
- **Identity, settings, tools, chat context and contracts.** This is a view
  within the registered Channels Space with its existing IRC configuration,
  storage, agent tools and contracts. No extra registration applies.
- **Help and tests.** Channels Help explains Comms and threads. The linked
  capture evidence uses local IRC fixtures. The test suite covers the inbox,
  thread navigation and navigator grouping.

## Native capture

- [Before](evidence/before.png) shows replies in the channel transcript and
  the expanded Other section.
- [Channel after](evidence/channel.png) shows the original requests with
  reply counts and Other collapsed.
- [Comms](evidence/comms.png) shows three live mentions from the local IRC
  fixture.
- [Thread selected](evidence/thread.png) shows the selected mention and the
  existing thread reply composer. The Comms count drops to two while the
  channel count stays at three.
