//! The newest message known in each IRC buffer, for reconnect catch-up.

use crate::model::{Anchor, Event, Line, LineKind, Target};

/// The anchor of `target`, by any spelling: anchors may arrive before the
/// server announces its case mapping.
pub(super) fn anchor<'a>(anchors: &'a [Anchor], target: &str) -> Option<&'a Anchor> {
    anchors
        .iter()
        .find(|anchor| name(&anchor.target).is_some_and(|name| name.eq_ignore_ascii_case(target)))
}

fn name(target: &Target) -> Option<&str> {
    match target {
        Target::Channel(name) | Target::Query(name) => Some(name),
        Target::Server => None,
    }
}

/// Keeps `next` when it is newer than the anchor held for its buffer.
pub(in crate::session) fn remember(anchors: &mut Vec<Anchor>, next: Anchor) {
    let Some(target) = name(&next.target) else {
        return;
    };
    match anchors
        .iter()
        .position(|held| name(&held.target).is_some_and(|held| held.eq_ignore_ascii_case(target)))
    {
        Some(at) if anchors[at].time < next.time => anchors[at] = next,
        Some(_) => {}
        None => anchors.push(next),
    }
}

/// Moves each buffer's anchor to the newest message among `events`, so a
/// reconnect asks only for what came after.
pub(in crate::session) fn track(anchors: &mut Vec<Anchor>, events: &[Event]) {
    let found: Vec<Anchor> = events
        .iter()
        .flat_map(event_lines)
        .filter(|line| {
            matches!(
                line.kind,
                LineKind::Privmsg | LineKind::Action | LineKind::Notice
            )
        })
        .map(|line| Anchor {
            target: line.buffer.target.clone(),
            id: line.id.clone(),
            time: line.time,
        })
        .collect();
    found
        .into_iter()
        .for_each(|anchor| remember(anchors, anchor));
}

fn event_lines(event: &Event) -> &[Line] {
    match event {
        Event::Line(line) | Event::Replayed(line) => std::slice::from_ref(line),
        Event::History { lines, .. }
        | Event::Backfill { lines, .. }
        | Event::Around { lines, .. } => lines,
        _ => &[],
    }
}
