//! The board's dock layout in buffer terms, as stored in the state file:
//! which conversations are open, in which tab groups and splits, and the
//! buffer list and member list docks' widths and visibility. Pure, so
//! saving, restoring and dropping stale conversations are tested without
//! GPUI.

use serde::{Deserialize, Serialize};
use subspace_client_irc::BufferKey;

/// The state-file key of the layout.
pub(crate) const KEY: &str = "irc.dock-layout";
/// A dock that has never been resized.
pub(crate) const DOCK_WIDTH: f32 = 240.;

/// How a split arranges its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Axis {
    /// Side by side.
    Horizontal,
    /// One above the other.
    Vertical,
}

/// One container of the centre: a split or a tab group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(crate) enum Pane {
    Split {
        axis: Axis,
        /// Each child with its size along the axis, when one was measured.
        children: Vec<(Pane, Option<f32>)>,
    },
    Tabs {
        buffers: Vec<BufferKey>,
        active: usize,
    },
}

/// A side dock: its width and whether it is open.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct Side {
    pub width: f32,
    pub open: bool,
}

impl Default for Side {
    fn default() -> Self {
        Self {
            width: DOCK_WIDTH,
            open: true,
        }
    }
}

/// The whole layout.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Layout {
    pub center: Option<Pane>,
    #[serde(default)]
    pub buffers: Side,
    #[serde(default)]
    pub members: Side,
}

/// The stored layout, or `None` when there is none or it cannot be read.
pub(crate) fn decode(text: Option<&str>) -> Option<Layout> {
    serde_json::from_str(text?).ok()
}

pub(crate) fn encode(layout: &Layout) -> String {
    serde_json::to_string(layout).unwrap_or_default()
}

impl Layout {
    /// Every conversation the layout holds, in order.
    #[cfg(test)]
    pub fn buffers(&self) -> Vec<BufferKey> {
        self.gather(|buffers, _| buffers)
    }

    /// The conversation each tab group displays, in order: every one on
    /// screen.
    pub fn shown(&self) -> Vec<BufferKey> {
        self.gather(|buffers, active| buffers.get(active..=active).unwrap_or_default())
    }

    fn gather(&self, pick: fn(&[BufferKey], usize) -> &[BufferKey]) -> Vec<BufferKey> {
        let mut found = Vec::new();
        if let Some(center) = &self.center {
            collect(center, pick, &mut found);
        }
        found
    }

    /// The displayed tab of the first tab group, which takes focus after a
    /// restore.
    pub fn first_active(&self) -> Option<BufferKey> {
        let mut pane = self.center.as_ref()?;
        loop {
            match pane {
                Pane::Tabs { buffers, active } => {
                    return buffers.get(*active).or(buffers.first()).cloned();
                }
                Pane::Split { children, .. } => pane = &children.first()?.0,
            }
        }
    }

    /// Drops the conversations `keep` refuses, such as those on networks
    /// that are no longer configured or removed from the list, and the tab
    /// groups and splits they leave empty.
    pub fn prune(&mut self, keep: &dyn Fn(&BufferKey) -> bool) {
        self.center = self.center.take().and_then(|center| prune(center, keep));
    }
}

/// The conversations `pick` chooses from each tab group under `pane`.
fn collect(pane: &Pane, pick: fn(&[BufferKey], usize) -> &[BufferKey], found: &mut Vec<BufferKey>) {
    match pane {
        Pane::Tabs { buffers, active } => found.extend(pick(buffers, *active).iter().cloned()),
        Pane::Split { children, .. } => {
            for (child, _) in children {
                collect(child, pick, found);
            }
        }
    }
}

/// `pane` with only the conversations `keep` accepts; `None` when nothing
/// is left. A split left with one child becomes that child.
fn prune(pane: Pane, keep: &dyn Fn(&BufferKey) -> bool) -> Option<Pane> {
    match pane {
        Pane::Tabs { buffers, active } => {
            let active_buffer = buffers.get(active).cloned();
            let kept: Vec<BufferKey> = buffers.into_iter().filter(|buffer| keep(buffer)).collect();
            let active = active_buffer
                .and_then(|buffer| kept.iter().position(|kept| *kept == buffer))
                .unwrap_or_default();
            (!kept.is_empty()).then_some(Pane::Tabs {
                buffers: kept,
                active,
            })
        }
        Pane::Split { axis, children } => {
            let mut kept: Vec<(Pane, Option<f32>)> = children
                .into_iter()
                .filter_map(|(child, size)| prune(child, keep).map(|child| (child, size)))
                .collect();
            match kept.len() {
                0 => None,
                1 => kept.pop().map(|(child, _)| child),
                _ => Some(Pane::Split {
                    axis,
                    children: kept,
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::{BufferKey, NetworkId, Target};

    use super::{Axis, Layout, Pane, Side, decode, encode};

    fn buffer(network: &str, target: Target) -> BufferKey {
        BufferKey {
            network: NetworkId(network.into()),
            target,
        }
    }

    /// Keeps the conversations on network `local`.
    fn local(buffer: &BufferKey) -> bool {
        buffer.network.0 == "local"
    }

    fn layout() -> Layout {
        let channel = buffer("local", Target::Channel("#ops".into()));
        let query = buffer("local", Target::Query("mira".into()));
        let gone = buffer("gone", Target::Channel("#old".into()));
        Layout {
            center: Some(Pane::Split {
                axis: Axis::Vertical,
                children: vec![
                    (
                        Pane::Tabs {
                            buffers: vec![gone.clone(), channel],
                            active: 1,
                        },
                        Some(320.),
                    ),
                    (
                        Pane::Tabs {
                            buffers: vec![query],
                            active: 0,
                        },
                        Some(200.),
                    ),
                    (
                        Pane::Tabs {
                            buffers: vec![gone],
                            active: 0,
                        },
                        None,
                    ),
                ],
            }),
            buffers: Side {
                width: 180.,
                open: true,
            },
            members: Side {
                width: 220.,
                open: false,
            },
        }
    }

    #[test]
    fn shown_is_the_displayed_tab_of_every_group() {
        assert_eq!(
            layout().shown(),
            [
                buffer("local", Target::Channel("#ops".into())),
                buffer("local", Target::Query("mira".into())),
                buffer("gone", Target::Channel("#old".into())),
            ],
            "the first group shows #ops, not the tab behind it"
        );
        assert!(Layout::default().shown().is_empty());
    }

    #[test]
    fn a_layout_round_trips_through_the_state_file() {
        let layout = layout();
        assert_eq!(decode(Some(&encode(&layout))), Some(layout));
        assert_eq!(decode(Some("not json")), None);
        assert_eq!(decode(None), None);
    }

    #[test]
    fn conversations_on_removed_networks_are_dropped_with_their_empty_groups() {
        let mut layout = layout();
        layout.prune(&local);
        let Some(Pane::Split { children, .. }) = &layout.center else {
            panic!("the split keeps two groups: {:?}", layout.center);
        };
        assert_eq!(children.len(), 2);
        assert_eq!(
            children[0].0,
            Pane::Tabs {
                buffers: vec![buffer("local", Target::Channel("#ops".into()))],
                active: 0,
            },
            "the active tab stays active"
        );
        assert_eq!(layout.members.width, 220., "docks keep their size");

        layout.prune(&|buffer| matches!(buffer.target, Target::Query(_)));
        assert_eq!(
            layout.buffers(),
            [buffer("local", Target::Query("mira".into()))],
            "a removed channel is dropped too"
        );
        layout.prune(&|_| false);
        assert_eq!(layout.center, None);
    }

    #[test]
    fn a_split_left_with_one_group_becomes_that_group() {
        let mut layout = layout();
        let Some(Pane::Split { children, .. }) = &mut layout.center else {
            panic!("split");
        };
        children.truncate(1);
        layout.prune(&local);
        assert!(matches!(layout.center, Some(Pane::Tabs { .. })));
        assert_eq!(layout.buffers().len(), 1);
        assert_eq!(
            layout.first_active(),
            Some(buffer("local", Target::Channel("#ops".into())))
        );
    }
}
