# Intent: glass capsule board tabs and title-bar spacing

The owner asked for two things in the window's top row:

- **Fix the spacing.** With horizontal board navigation on macOS, the
  Subspace logo sat close after the traffic lights, and the board tabs were
  flat, square-cornered blocks that ran from the window's top edge to the
  bar's hairline and butted against each other. The 30 px row was shorter
  than the traffic lights' 40 px band, so the lights hung below the row's
  centre line.
- **Lozenge tabs in the Liquid Glass style.** The board tabs, and the
  vertical rail's current item to match, become full capsules. The current
  board's capsule is glass, as in Apple's macOS 26 Liquid Glass: a
  translucent fill, a light rim that is stronger at the top, and a soft
  shadow that lifts it off the bar. The other tabs have no fill.

## Decisions along the way

1. The first request was "lozenges like the vertical rail". The reference
   image did not arrive.
2. The owner then asked for a squircle (continuous-curvature) rounded
   rectangle like the Wake app's sidebar rows: a radius of about a quarter of
   the item height and a low-contrast grey fill with no border or shadow. The
   reference showed a light grey sidebar. Its selected row, "All Sessions 22",
   was a wide soft rectangle with generous horizontal padding, and "Starred"
   below it had no fill. GPUI has no corner smoothing or superellipse
   primitive, so this would have been an ordinary rounded rectangle.
3. The owner rejected the squircle variant: "actually no, full lozenge liquid
   glass style". This record describes that final design. The Wake screenshot
   is not committed.
