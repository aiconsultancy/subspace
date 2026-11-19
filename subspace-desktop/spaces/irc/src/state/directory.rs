//! Every network's public channels, from its last `LIST`
//! ([`Event::ChannelList`](subspace_client_irc::Event::ChannelList)). The
//! hub keeps them in a cache file beside the state file, so the list shows
//! at once on the next start, and replaces a network's list when a new one
//! arrives.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use subspace_client_irc::{ListedChannel, NetworkConfig, NetworkId};

/// The cache file beside the state file.
pub(crate) const CACHE_FILE: &str = "irc-channel-list.json";
/// The most channels kept per network: the busiest, so a large network's
/// list stays quick to draw and search.
pub(crate) const KEPT: usize = 500;

/// One network's public channels and when they were listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Directory {
    pub fetched: DateTime<Utc>,
    pub channels: Vec<ListedChannel>,
}

/// The public channels of every configured network that has listed them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Directories {
    by_network: BTreeMap<NetworkId, Directory>,
    /// Changed since the hub last saved them.
    changed: bool,
}

impl Directories {
    /// The cached lists; none when there is no cache or it cannot be read.
    pub fn decode(text: Option<&str>) -> Self {
        let by_network = text
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_default();
        Self {
            by_network,
            changed: false,
        }
    }

    pub fn encode(&self) -> String {
        serde_json::to_string(&self.by_network).unwrap_or_default()
    }

    /// `network`'s channels, when it has listed them.
    pub fn get(&self, network: &NetworkId) -> Option<&Directory> {
        self.by_network.get(network)
    }

    /// Replaces `network`'s list with `channels`, listed at `now`: its
    /// [`KEPT`] busiest channels, by name.
    pub fn replace(
        &mut self,
        network: &NetworkId,
        mut channels: Vec<ListedChannel>,
        now: DateTime<Utc>,
    ) {
        channels.sort_by_key(|channel| std::cmp::Reverse(channel.users));
        channels.truncate(KEPT);
        channels.sort_by_cached_key(|channel| channel.name.to_lowercase());
        self.by_network.insert(
            network.clone(),
            Directory {
                fetched: now,
                channels,
            },
        );
        self.changed = true;
    }

    /// Drops the lists of networks that are no longer configured.
    pub fn retain(&mut self, configs: &[NetworkConfig]) {
        let before = self.by_network.len();
        self.by_network
            .retain(|network, _| configs.iter().any(|config| &config.id == network));
        self.changed |= self.by_network.len() != before;
    }

    /// True once after the lists changed, for the hub to save them.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::{ListedChannel, NetworkId};

    use super::{Directories, KEPT};
    use crate::fixtures::{at, config, network_id};

    fn listed(name: &str, users: u32) -> ListedChannel {
        ListedChannel {
            name: name.into(),
            users,
            topic: format!("About {name}"),
        }
    }

    #[test]
    fn a_list_survives_the_cache_and_keeps_name_order() {
        let mut directories = Directories::default();
        directories.replace(
            &network_id(),
            vec![listed("#zebra", 3), listed("#Alpha", 9), listed("#beta", 1)],
            at(5),
        );
        assert!(directories.take_changed());
        assert!(!directories.take_changed(), "reported once");

        let restored = Directories::decode(Some(&directories.encode()));
        let directory = restored.get(&network_id());
        assert_eq!(directory.map(|known| known.fetched), Some(at(5)));
        let names: Vec<&str> = directory
            .map(|known| {
                known
                    .channels
                    .iter()
                    .map(|channel| channel.name.as_str())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(names, ["#Alpha", "#beta", "#zebra"]);
        assert_eq!(restored, directories, "the topic and counts survive too");
    }

    #[test]
    fn an_unreadable_cache_is_empty() {
        assert_eq!(Directories::decode(Some("[1,2]")), Directories::default());
        assert_eq!(Directories::decode(None), Directories::default());
    }

    #[test]
    fn only_the_busiest_channels_are_kept() {
        let mut directories = Directories::default();
        let many = (0..=KEPT)
            .map(|index| listed(&format!("#c{index:04}"), u32::try_from(index).unwrap_or(0)))
            .collect();
        directories.replace(&network_id(), many, at(0));
        let kept = &directories
            .get(&network_id())
            .map(|known| known.channels.clone())
            .unwrap_or_default();
        assert_eq!(kept.len(), KEPT);
        assert!(
            !kept.iter().any(|channel| channel.name == "#c0000"),
            "the quietest goes"
        );
    }

    #[test]
    fn a_removed_network_loses_its_list() {
        let mut directories = Directories::default();
        directories.replace(&NetworkId("gone".into()), vec![listed("#a", 1)], at(0));
        directories.replace(&network_id(), vec![listed("#b", 1)], at(0));
        directories.take_changed();
        directories.retain(&[config()]);
        assert!(directories.get(&NetworkId("gone".into())).is_none());
        assert!(directories.get(&network_id()).is_some());
        assert!(directories.take_changed());
    }
}
