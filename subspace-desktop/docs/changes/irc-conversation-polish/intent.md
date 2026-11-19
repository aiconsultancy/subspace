# Intent: IRC conversation polish

The owner used the IRC board in Subspace v0.2.0 and asked for it to behave
more like Slack:

1. **Hover.** The message under the cursor should be highlighted. A row was
   filled only when it mentioned you, and hover only revealed the time on a
   continuation line.
2. **Shift+Return.** It should add a line break, and Return alone should
   send. The source suggested this already worked, so the first step was to
   reproduce it. It did not work: Shift+Return added the line break and then
   sent the draft at once. GPUI Kit reports Shift+Enter as a `PressEnter`
   after inserting the line break, and the shared chat input sent on every
   `PressEnter`. The hint said only "Enter to send", so nobody could have
   discovered the key either.
3. **Members panel.** With Members open, a darker strip sat between the
   member list and the window's right edge. The list had a fixed `14rem`
   width inside the right dock, which is 240 px by default, resizable and
   restored from the saved layout. Any saved width above 224 px showed the
   strip.
4. **Removing items.** A private conversation's right-click menu should offer
   a way to remove it from the list, and it should stay gone after a restart.

## Decisions

1. **Shared fix.** Shift+Enter is fixed in the shared `ChatInput`, so the
   agent chat's input stops sending on Shift+Enter too.
2. **Up and Down in a multi-line draft.** The captain's decision: they move
   the cursor between lines. Sent-line recall applies only while the draft is
   empty or one line. Tab still completes in any draft. A recalled message of
   several lines counts as part of the recall until it is edited; otherwise
   recalling one would stop the walk to older lines and the way back to the
   draft.
3. **Hint.** `Return to send · Shift+Return for a new line` on macOS, `Enter`
   elsewhere, after `Markdown ·` in a Markdown input.
4. **Hover colour.** The list's hover token (`hair`) is darker than the
   transcript's `surface` in Cupertino Dark and equal to it in One Dark, so it
   does not show there. The row takes the foreground at 6 % opacity instead,
   which lightens a dark theme and darkens a light one by a similar step. A row
   that mentions you keeps its `select` fill, as a selected list row does.
5. **Member list.** The list fills its host; the dock owns the width. This
   also fixes a saved layout that is already wider than the list was.
6. **Labels.** **Close conversation** for a private conversation: it names
   the object and suggests nothing is deleted, unlike a bare **Remove**. The
   toolbar's **More** menu's **Close private messages** takes the same label,
   because a menu item uses the same verb everywhere it appears. A channel we
   are no longer in offers **Remove from list** instead of **Leave
   channel**, because the label should predict the result and there is
   nothing to leave.
7. **Settled defaults** (Defiant, tracker lead; accepted by the captain):
   - A private conversation is hidden, and a new private message from that
     person brings it back.
   - A channel's menu offers **Leave channel**, which parts and removes it.
   - The server item cannot be removed.
   - A removed item stays out of the list after a restart, a reconnect or a
     history replay.
   - Multi-line sending is unchanged.
8. **Configured channels.** A channel in the network's `channels` setting is
   joined on every connect. To keep a left channel out after a restart, the
   board does not ask the runtime to join it; to keep it out after a
   reconnect, removing it sends the runtime a new `Forget` command, which
   also covers a channel we were kicked from. For a private conversation the
   same command makes the session forget it, so a service's notices stop
   reopening it. The setting
   itself is not edited: boards cannot write their settings file outside
   Settings, and a changed network configuration restarts its connection.
   Joining the channel again, or adding it to the setting in Settings, brings
   it back. A typed `/part` keeps its old behaviour.
9. **A channel we are no longer in** (after a kick or a typed `/part`) can be
   removed with **Remove from list**. A joined channel cannot be left while
   its network is disconnected, because the part needs the server. There is
   no undo or confirmation: joining the channel again recovers it.
10. **Matching names.** Removals match ignoring ASCII case, on the board and
    in the runtime, not with RFC 1459 folding or Unicode case: the board does
    not know each network's case mapping, and folding would hide a different
    channel or nick on a network with ASCII case mapping. The closing time is
    the newest message someone else sent, which the server stamped; a
    private conversation without one records no time, so any message brings
    it back, because our clock cannot be compared with the server's.
11. **Our own messages.** Our own message in a closed private conversation,
    such as `/msg mira …` typed in a channel or sent from another client,
    brings it back like any new message. A message sent and then closed
    within the echo's round trip comes back the same way; treating our own
    echo differently would hide a `/msg` we just sent.
