# Agent activity mark

## Intent

Show whether an agent is working alongside its reported phase and elapsed time.
The phase describes its current activity and carries no estimate of completion.

## Specification

Two highlights travel around the stationary Subspace mark and meet in its
centre. The motion repeats while the agent is thinking, using a tool or writing
a reply. Queued, waiting, retry, stale and finished lines have steady icons.
The clock and spoken status continue to describe the actual request.

The same mark takes the place of the spinner on the top-right Reload button
while the current Space is busy. It sits at the top left in the existing
product mark's space. The Reload button retains its own icon and reports
that it is unavailable while the Space is busy. The status bar keeps its text and leaves the
activity mark to the toolbar while the Space is visible. The transcript and
the toolbar follow their own activity states. The mark stays steady when
GPUI reports reduced motion.

## Delivery

The Framework's `activity_mark::ActivityMark` draws the product shape with
native GPUI paths and uses GPUI's animation clock, capped at 24 frames per
second. The Component gallery shows an active and a steady mark. The Subspace
desktop product opts its toolbar into the branded animation.

## Tasks and checks

- Check the motion geometry and the split between active and waiting phases.
- Exercise Channels status lines and inspect native captures with fixture data
  in Twilight Two.
- Confirm that the Component gallery catalogue registers the shared mark.

## Evidence

The captures use Twilight Two with a loopback IRC fixture and a GitHub
fixture. The same Channels request appears before and after the change.

- [Before](evidence/before.png) shows the five-step bar beside each request.
- [After](evidence/after.png) shows the animated mark on the tool-use line and a
  steady pause glyph on the waiting line.
- [Actions reloading](evidence/reloading.png) shows the accent logo at the top
  left, the existing Reload glyph, and loading text in the status bar.
