# Comment authoring native evidence

[Delivery](../delivery.md) · [Evidence contract](../spec.md#11-delivery-and-evidence)

## PR B

All four frames were recaptured on 16 November 2026 after rebasing onto
`8750fba735228fb4a1832b38aa9cd5a9d15b95c4`. They show the current Cupertino
Dark macOS button tokens. Attach image…, Retry and Remove retain the shared
Action role for labelled board-content actions. The links below resolve the
PNG bytes in the same commit as this record; the PR body pins that commit.
The delivery subsequently rebases onto `e259f47` (#47, IRC reply counts);
that merge changes none of these visible Issues states, so these fresh frames
remain the final delivery evidence.

| Frame | State |
| --- | --- |
| [attachment-uploading-1440-dark.png](attachment-uploading-1440-dark.png) | Measured transfer progress, Remove available, submit held until Ready |
| [attachment-failed-1440-dark.png](attachment-failed-1440-dark.png) | One failed image with Retry/Remove; another image remains Ready |
| [attachment-posted-1440-dark.png](attachment-posted-1440-dark.png) | Image-only comment returned by the fixture, rendered through private grants |
| [attachment-composer-960-dark.png](attachment-composer-960-dark.png) | Actual 960 px composition with two retained images |

The PR B frames use an isolated fixture HOME, capture-feature app,
Cupertino Dark, Noto Sans/Liberation Mono, owned Xvfb and unedited
`ffmpeg -f x11grab` output. Startup updates are disabled with the saved `0`
preference. An owned X11 clipboard selection serves synthetic PNG bytes directly
from memory; native Control+V enters the real composer's paste path. The
picker path is covered separately by a native headless prompt test. Uploading
uses the delayed PUT fixture; failure uses a synthetic Complete 503 while a
second image is kept. Retry recovers the first image without retransferring it.
The posted-image capture recorded two reservations, two transfers, two
references and one saved image-only comment: one of each media operation per
intended image. No upload/grant URL is
visible. Every frame was inspected at native size. The 960 px frame resizes
the actual native window; it is not a crop. Each runner closes its clipboard,
app, Xvfb and fixture server. Human NETT dev and macOS acceptance are separate.

## PR A

| Frame | State |
| --- | --- |
| before-52d2768-1440-dark.png | Base discussion, before authoring |
| [composer-empty-1440-dark.png](composer-empty-1440-dark.png) | Empty composer beneath timeline |
| [composer-draft-1440-dark.png](composer-draft-1440-dark.png) | Retained multiline Markdown draft and submit hint |
| [composer-posting-1440-dark.png](composer-posting-1440-dark.png) | Single in-flight submit, frozen input |
| [composer-failed-1440-dark.png](composer-failed-1440-dark.png) | Recognized pre-save refusal, text kept and Retry |
| [composer-unknown-1440-dark.png](composer-unknown-1440-dark.png) | Fence copy, Retry and Discard, frozen text and accurate recovery hint |
| [reply-composer-1440-dark.png](reply-composer-1440-dark.png) | Reply editor names its root author |
| [composer-draft-960-dark.png](composer-draft-960-dark.png) | Actual 960 px window, draft controls fit |
| [reply-draft-held-1440-dark.png](reply-draft-held-1440-dark.png) | Closed reply retains a compact Open/Discard row |

Build with `mise exec -- cargo build -p subspace --features subspace-shell/capture`.
Use `scripts/nett_test_server.py --rows design`, the debug numeric-loopback
API/WorkOS overrides, isolated HOME/XDG directories, DejaVu Sans/Mono and
`app.window = "[0,0,1440,900]"`, with the theme unset. Disable updates and
chat and enable Issues alone. Settings sign-in uses the fictional device flow.

On the owned Xvfb display the app capture steps open Settings, choose Issues,
sign in, close Settings, open TST-1612 and scroll to the timeline's end.
Capture the empty state, then click the text area and type
“I checked the fixture. This is ready for review.” For posting use scenario
`comment-posting`; for clear failure use `comment-refused`; for unknown use
`comment-drop`. Press `ctrl-enter` and capture while the respective state is
visible. Reply uses the last root's Reply control. The narrow frame resizes
the owned native window with XResizeWindow before capture; it is not a crop
of a wide layout. `ffmpeg -f x11grab -draw_mouse 0 -framerate 2` records each
unaltered PNG at scale 1. All people, comments and image bytes are synthetic.

The PR A review recaptures the wide draft with a second line,
“I also checked the keyboard flow.”, and the unknown state with
“Choose Retry or Discard.” The other six original frames are unchanged.
The additional held-reply frame types a reply and presses Escape before
capture, showing the new recovery row. These captures use the same owned
loopback/Xvfb tooling, restoring the isolated window to 1440 × 900 first.

These frames are local evidence, not the later live NETT dev acceptance.
