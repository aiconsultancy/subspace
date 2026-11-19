# Fleet Console: smooth under load

## Intent

Fleet Console ran badly inside Subspace's web view (WKWebView) on the
owner's machine under load, while the Canvas 2D mock-up stayed smooth in a
browser. The owner asked to make it smooth, measured in WebKit, with a
frame-time readout, cheap wins, and an adaptive quality governor, keeping
the look at the top quality level.

## Spec

- **Rendering.** Colour draws straight into the canvas at the quality
  level's internal resolution; emissive light draws in a second pass into a
  bloom target of a quarter (or a sixteenth) of the CSS pixel count, is
  blurred there and blended over the canvas (screen for dark themes,
  multiply-tint for light ones). This removes the full-resolution
  two-attachment RGBA16F scene target and the full-resolution composite
  read. Bloom targets stay half-float where available; they are small.
- **Overdraw.** Stroke-only rings larger than 18 screen pixels draw as 8 to
  64 tight slices of the exact ring, each owning a hard angular range, so
  the stroke is unchanged and seamless. The radar sweep's quad covers only
  its wedge. The gate's quad is tighter, and without the event horizon its
  inside is discarded early.
- **Labels.** The overlay redraws only when its signature (camera, every
  label's text, position and style) changes; a moving camera redraws
  without building it. Off-screen labels are skipped; font, widths, fits
  and colours are cached. Crew views, initials and layout point lists are
  memoized.
- **Rates.** Ambient motion is 30 fps, 20 fps without focus or after 45
  seconds without input; warps and packets run at the display rate.
  Reduced-motion clock ticks redraw only the labels. Alternate ambient
  frames reuse the previous light.
- **Governor.** Each second of animated frames is judged against its target
  interval (display rate, or ambient rate) and the frame budget (CPU time,
  plus GPU time where timer queries exist). One slow second steps down one
  level: scene resolution 2x, 1.5x (no event horizon below this), 1.25x, 1x,
  then one bloom level, no bloom, and 20 fps ambient. Five seconds of
  headroom and an eight-second cooldown since the last step down step back
  up. The display rate is the median callback interval snapped to a
  standard rate. Nothing is persisted; reduced motion is unchanged.
- **Context loss.** A lost WebGL context gets three seconds to be restored
  (and is rebuilt) before Fleet Console switches to Canvas 2D, which now
  also follows the quality level.
- **Readout.** `?perf=1` shows median and p95 frame time, CPU time, GPU time
  where measurable, frame rate, label redraws per second, the level, the
  scene's pixel size and the governor's verdict. `?quality=N` pins a level.
  Inside Subspace, where the address cannot be changed, the **Frame-time
  readout** setting (Off by default) or Shift+P with the map focused shows
  it; Shift+P flips it until the setting next changes.

## Delivery

Measured on an Apple M1 Max at device scale factor 2 and a 1500×950
viewport, sample mode, comparing the build before this change with this
one at level 0 (the full look). Scenarios: five seconds of the fleet tier
idle, three warps into Galactica and back out, and five seconds of the crew
tier.

| GPU time per drawn frame, median (p95), Chromium timer queries on ANGLE Metal | Before | After, level 0 | After, level 6 |
| --- | --- | --- | --- |
| Fleet idle | 1.18 ms (1.59) | 0.63 ms (0.88) | 0.15 ms (0.23) |
| Warps | 1.40 ms (1.88) | 0.92 ms (1.33) | 0.23 ms (0.35) |
| Crew tier | 1.02 ms (1.21) | 0.58 ms (0.87) | 0.24 ms (0.31) |

| WebKit 26 (Playwright), mean per frame callback with the GPU work forced to finish | Before | After, level 0 |
| --- | --- | --- |
| Fleet idle | 2.88 ms | 1.89 ms |
| Warps | 3.31 ms | 2.79 ms |
| Crew tier | 2.41 ms | 1.79 ms |

| CPU per drawn frame, mean, Chromium with 4x CPU throttling | Before | After, level 0 |
| --- | --- | --- |
| Fleet idle | 1.78 ms | 1.31 ms |
| Warps | 2.56 ms | 2.34 ms |
| Crew tier | 1.59 ms | 1.13 ms |

WebKit frame intervals stayed at a 17 ms median in every case; p95 was 18,
21 and 30 ms before and 19, 19 and 18 ms after (unloaded), and 18 to 22 ms
with twelve busy-loop processes on the ten cores, before and after. On this
machine the governor stayed at level 0 throughout, including under that
load. Labels redraw about once a second while the map idles, instead of on
every frame. In one of three early loaded runs the old build lost its WebGL
context and switched to Canvas 2D for good; this did not recur.

Screenshots at device scale factor 2 in WebKit, dark and light, fleet, ship
and mid-warp, matched the previous build at level 0 apart from animation
phase. Level 5 (no bloom, 1x scene) is flatter but keeps every element.

Not measured: Subspace's own WKWebView (Playwright's WebKit build is the same
engine in a different host), the cost of compositing the two canvases (out of
the page's reach), GPU time in WebKit (it has no timer queries), the owner's
machine, and GPU contention from other applications.

## Tasks

- [x] Frame-time readout behind `?perf=1`; `?quality=N`
- [x] WebKit benchmark at DPR 2: fleet idle, warps, crew tier, before and after, with CPU load
- [x] Two-pass rendering at an adaptive internal resolution; RGBA8 canvas
- [x] Tight ring slices, sweep and gate quads
- [x] Label redraw only on change; cached measurement
- [x] Lower ambient rate without focus or input; light reuse on ambient frames
- [x] Adaptive quality governor
- [x] Tests for the governor, label dirty tracking and batching
- [x] Readout inside Subspace: a setting and Shift+P on the map
- [ ] Confirm on the owner's machine in Subspace with the readout on
