# Intent: IRC mention notifications

The owner asked for notifications in the IRC board when someone tags them
This is part of the Issues and Channels initiative.
In 0.2.0 the tabs and buffer list show unread and mention badges, but nothing
appeared outside the app window.

The captain who leads the tracker settled these defaults:

- An operating-system notification that names the sender and the channel,
  with a sound.
- It fires when someone tags the signed-in person, and for private messages,
  only when that conversation is not in view or the app is not in front.
- It never fires for the person's own messages, or for history loaded on
  connect or reconnect, including `chathistory` and event playback.
- Clicking it brings Subspace to the front on that conversation.
- macOS is required and Linux is best effort. The dock badge is out of scope.
  The unread and mention badges stay as they are.

## What already existed

The board already raised a notification request for mentions and private
messages in buffers that were not in view. Choosing the notification opened
that buffer. Three things stopped it doing what was asked:

1. **Delivery was Off by default.** Nothing reached the operating system
   unless the person found **Settings > Notifications > IRC** and changed it.
   Worse, every Settings Save stored each board's Off default, even when it
   changed nothing about notifications. So anyone who had added a network
   in Settings had Off stored as if they had chosen it.
2. **The OS text was generic.** Every OS post read "New activity is
   available." with the product name as its title.
3. **Some old lines could still notify.** A live line that the read marker
   already covered, such as one another client had read, could notify. So
   could a notice in the **Server** buffer that names the person, such as a
   services notice on connect. Neither raises the unread badges.

## Decisions

1. **Reuse the badge rules.** A line notifies only when it counts as unread
   (the rule behind the badges, in `state/counts.rs`) and it either mentions
   the person (the same match that makes the mention badge) or is in a private
   conversation. There is no second matcher.
2. **History never notifies.** Only live lines can raise a notification.
   `chathistory` batches, including those `draft/event-playback` fills and
   those the server sends without a request on join, reach the board as
   history. The client already separates them by their batch markers. A live
   line that the read marker covers by its server time does not notify.
   There is no wall-clock cut-off.
3. **Name who and where, never what was said.** The OS title is one sentence:
   "alice mentioned you in #subspace" or "carol sent you a private message".
   The body is the network's name. The message text stays out, because OS
   notifications can show on a locked screen and in the notification
   history.
4. **Boards opt in to descriptive OS text.** A board passes a `Notice`. Only a
   notice built with `Notice::described` puts its text in the OS
   notification. Every other notice keeps the generic OS text. Web boards
   still pass page-supplied text, so their OS posts stay generic.
5. **IRC delivery is on by default.** A board may declare notification
   defaults in its settings metadata. IRC declares **OS** delivery with the
   **Glass** sound. These apply until the person saves their own choice in
   Settings. Web boards still declare nothing, so they start **Off**.
   Settings now saves a board's choice only when it changed, with a marker.
   An unmarked Off, Standard and Silent is the old automatic default, so
   people upgrading from 0.2.1 or earlier get the new default too. A
   deliberate earlier choice of exactly that cannot be told apart and also
   moves to OS. Any other earlier choice is kept.
6. **The sound is Subspace's.** GPUI's native post sets no sound, so the
   operating system plays none of its own. Subspace plays the chosen system
   sound beside the post, as it already did for the other sound choices. On
   macOS the default is Glass, the default sound of the app's other alerts.
   Linux has no sound choices, so it stays silent.
7. **A click brings the app forward.** The click asks the platform to activate
   the app as well as its window. macOS already does this for its own
   notifications. Linux ignores the request, so there the window manager
   decides.
8. **A bouncer's playback is not news.** ZNC replays its buffers on connect
   in a `znc.in/playback` batch, not `chathistory`. The client marks those
   lines as replayed. The board appends and counts them like live lines, so
   the badges show them, but they never notify.
9. **A split is in view.** Every conversation on screen stays quiet: the
   focused one and the displayed tab of each other group in a split. A tab
   behind another one is not on screen, so it notifies. Only the focused
   conversation reads as lines arrive, as before.

## Known limits

- A server that replays old lines as plain live messages, with neither a
  batch nor a read marker, can still notify for them. Telling them apart
  would need a wall-clock guess, which the settled defaults rule out.
- Each mention or private message posts its own notification and sound, so
  a burst while you are away gives a burst of banners.
- A ZNC bouncer's control messages, such as `*status` on attach, are private
  messages and notify.
- A read marker set from a line timed by the local clock, such as a local
  echo without `echo-message`, can hide a mention timed by a server clock
  that runs behind. The unread badges already share this.
