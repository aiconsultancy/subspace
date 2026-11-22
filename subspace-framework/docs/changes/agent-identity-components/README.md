# Agent identity components

## Intent

Channels could only mark a fleet agent with a muted pill beside its name,
and drew agents with the same square initials as people, because the shared
components had no way to say "this is an agent of this rank". The approved
Channels mock-up marks captains in blue and the rear admiral in the accent,
with a round avatar that carries the rank's glyph. This change adds that
language to the Framework, additively, so any Space can use it.

## Spec

- `badge::Pill::hue(Tone)` draws a pill in any theme tone, with the same
  tinted fill as the semantic tones, and takes precedence over
  `Pill::tone`. `PillTone` is unchanged: adding variants to a public enum
  that is not `#[non_exhaustive]` would break a caller's exhaustive match.
- `avatar::agent(id, AgentRank, name)` returns an `AgentAvatar`: a circle
  as large as `avatar::sender` with a one-pixel ring, a faint fill and the
  rank's Lucide glyph, all in the rank's tone (captain `Navigation2` in
  `Tone::Blue`, rear admiral `Diamond` in `Tone::Accent`). `.small()` makes
  it as large as `avatar::member`. Its role is image and its accessible name
  is the name and the rank, such as *aurora-lyra, captain*.
  `AgentRank` is `#[non_exhaustive]`, so a crew rank can follow without a
  breaking change.
- `AgentAvatar::crew(busy, total)` draws GPUI Kit's `ProgressCircle` around
  a smaller face: one arc for the busy share, in the rank's tone, and the
  count in the accessible name. The mock-up's ring of separate arcs, one per
  crew member, would need custom drawing; this is the simpler ring.
- `avatar::external(id, name)` is a relayed person: up to two initials in
  muted text inside a dashed muted outline, the size and shape of
  `avatar::sender`, with no fill.
- `avatar::crew_meter(id, busy, total)` is a row of dots, busy members
  filled green and idle ones a muted ring, with the meter role and the name
  *3 of 4 crew busy*. A crew larger than eight shows `busy/total` instead.
- Every colour comes from `Tokens`, so each theme draws them in its own
  blue, accent, green and muted.

## Delivery

- `board-kit/dashboard/src/badge/pill.rs`, `avatar.rs`, `avatar/agent.rs`,
  `avatar/external.rs`, `avatar/crew.rs`, with unit tests.
- Component gallery: the `badge::Pill` card's two hue states, and the
  `avatar::agent`, `avatar::external` and `avatar::crew_meter` cards.
- [UI component catalogue](../../ui-components.md): the `avatar` and
  `badge` entries and the gallery names.

## Tasks

- [x] Pill hues, agent avatar, crew ring, external avatar and crew meter.
- [x] Gallery cards, catalogue entries and tests.
- [ ] Use `external` and `crew_meter` in Channels once the API reports relayed
      people and crew activity.
