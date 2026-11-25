# Channels preview status colour

## Intent

Make issue and pull request previews easier to scan in Channels by concentrating
status colour in the badge.

## Specification

On hover or keyboard focus, ordinary issues and pull requests use the same
outlined popover frame as trackers. State badges keep their colour, icon and
label. The compact tiles continue to use their status icons.

## Delivery

Both previews use the outline treatment supplied by the shared Reference Card
component. The existing content and hover interaction are reused.

## Verification

- [x] Passed all 14 Channels hover tests.
- [x] Compared issue and pull request hover captures before and after in Twilight Two.
