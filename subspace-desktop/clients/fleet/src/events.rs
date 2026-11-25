//! Incremental framing of the Fleet API's server-sent events. A frame is an
//! `event:` line, one or more `data:` lines of JSON and a blank line. Each
//! frame is bounded; unknown event types are skipped.

use std::collections::VecDeque;

use serde::Deserialize;

use crate::model::{ActivityEvent, FleetOverview, Host, ShipDetail};

/// A ship detail with a large crew stays far below this.
const MAX_FRAME_BYTES: usize = 2 * 1024 * 1024;

/// One event from `GET /api/v1/events`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FleetEvent {
    /// The overview: sent on connect and when it changes.
    Fleet(Box<FleetOverview>),
    /// One ship's detail: one per ship on connect, then on change.
    Ship(Box<ShipDetail>),
    /// A ship's detail was read again without change.
    ShipSampled { id: String, detail_updated_at: i64 },
    /// The fleet VM's figures, every ten seconds.
    Host(Box<Host>),
    /// One line of crew activity.
    Activity(Box<ActivityEvent>),
    /// Every fifteen seconds.
    Heartbeat { at: i64 },
}

/// The stream broke its framing: an oversized frame or text that is not
/// UTF-8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FramingError;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sampled {
    id: String,
    #[serde(default)]
    detail_updated_at: i64,
}

#[derive(Deserialize)]
struct Heartbeat {
    #[serde(default)]
    at: i64,
}

/// Bytes in, events out.
#[derive(Default)]
pub(crate) struct Frames {
    pub(crate) ready: VecDeque<FleetEvent>,
    line: Vec<u8>,
    name: String,
    data: Vec<u8>,
    /// A CR ended the last line; an LF that follows it ends nothing.
    skip_lf: bool,
    started: bool,
    size: usize,
}

impl Frames {
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<(), FramingError> {
        for &byte in chunk {
            if std::mem::take(&mut self.skip_lf) && byte == b'\n' {
                continue;
            }
            self.size += 1;
            if self.size > MAX_FRAME_BYTES {
                return Err(FramingError);
            }
            if matches!(byte, b'\r' | b'\n') {
                self.skip_lf = byte == b'\r';
                self.end_line()?;
            } else {
                self.line.push(byte);
            }
        }
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), FramingError> {
        let mut line = std::str::from_utf8(&self.line).map_err(|_| FramingError)?;
        if !std::mem::replace(&mut self.started, true) {
            line = line.trim_start_matches('\u{feff}');
        }
        if line.is_empty() {
            self.ready.extend(event(&self.name, &self.data));
            self.size = 0;
            self.name.clear();
            self.data.clear();
        } else {
            let (field, value) = line.split_once(':').unwrap_or((line, ""));
            let value = value.strip_prefix(' ').unwrap_or(value);
            match field {
                "event" => {
                    self.name.clear();
                    self.name.push_str(value);
                }
                "data" => {
                    self.data.extend_from_slice(value.as_bytes());
                    self.data.push(b'\n');
                }
                _ => {}
            }
        }
        self.line.clear();
        Ok(())
    }
}

/// The event a finished frame holds. A type this version does not know,
/// or data that does not read as that type, is skipped.
fn event(name: &str, data: &[u8]) -> Option<FleetEvent> {
    let data = data.strip_suffix(b"\n")?;
    match name {
        "fleet" => parse(data).map(|fleet| FleetEvent::Fleet(Box::new(fleet))),
        "ship" => parse(data).map(|ship| FleetEvent::Ship(Box::new(ship))),
        "ship-sampled" => parse(data).map(|sampled: Sampled| FleetEvent::ShipSampled {
            id: sampled.id,
            detail_updated_at: sampled.detail_updated_at,
        }),
        "host" => parse(data).map(|host| FleetEvent::Host(Box::new(host))),
        "activity" => parse(data).map(|activity| FleetEvent::Activity(Box::new(activity))),
        "heartbeat" => parse(data).map(|beat: Heartbeat| FleetEvent::Heartbeat { at: beat.at }),
        _ => None,
    }
}

fn parse<T: serde::de::DeserializeOwned>(data: &[u8]) -> Option<T> {
    serde_json::from_slice(data).ok()
}

#[cfg(test)]
mod tests {
    use super::{FleetEvent, Frames, FramingError, MAX_FRAME_BYTES};

    #[test]
    fn frames_split_across_chunks_and_line_endings_become_events() {
        let mut frames = Frames::default();
        let text = "\u{feff}event: heartbeat\r\ndata: {\"at\":7}\r\n\r\n: comment\n\
                    event: fleet\ndata: {\"name\":\"Example\",\"ships\":[]}\n\n\
                    event: teleport\ndata: {}\n\n\
                    event: ship-sampled\ndata: {\"id\":\"s\",\"detailUpdatedAt\":3}\n\n";
        for chunk in text.as_bytes().chunks(5) {
            frames.push(chunk).unwrap();
        }
        let events: Vec<_> = frames.ready.drain(..).collect();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], FleetEvent::Heartbeat { at: 7 });
        assert!(matches!(&events[1], FleetEvent::Fleet(fleet) if fleet.name == "Example"));
        assert_eq!(
            events[2],
            FleetEvent::ShipSampled {
                id: "s".into(),
                detail_updated_at: 3
            }
        );
    }

    #[test]
    fn ship_events_keep_task_revision_with_bounded_roster_text() {
        let mut frames = Frames::default();
        frames.push(b"event: ship\ndata: {\"id\":\"s\",\"crewMembers\":[{\"id\":\"wren\",\"contexts\":[{\"current\":{\"id\":\"task_1\",\"status\":\"running\",\"updatedAt\":42,\"progress\":\"bounded\"}}]}]}\n\n").unwrap();
        let Some(FleetEvent::Ship(ship)) = frames.ready.pop_front() else {
            panic!("expected a ship event");
        };
        let task = ship.crew_members[0].current_task().unwrap();
        assert_eq!(task.updated_at, Some(42));
        assert_eq!(task.progress, "bounded");
    }

    #[test]
    fn oversized_frames_and_invalid_text_break_the_stream() {
        let mut frames = Frames::default();
        assert_eq!(
            frames.push(&vec![b'a'; MAX_FRAME_BYTES + 1]),
            Err(FramingError)
        );
        let mut frames = Frames::default();
        assert_eq!(frames.push(b"data: \xff\n"), Err(FramingError));
        let mut frames = Frames::default();
        frames.push(b"event: ship\ndata: not json\n\n").unwrap();
        assert!(frames.ready.is_empty());
    }
}
