//! Which fleet a channel belongs to, so the navigator can group a network's
//! channels by fleet: a fleet's Starbase, command and ship channels as the
//! roster names them, then the channels its initiatives name. A channel that
//! two fleets on one network claim belongs to neither, as nothing is
//! guessed.

use std::collections::{HashMap, HashSet};

use subspace_api_client::model::{Fleet, MemberInitiative};

use super::{ChannelKind, Names};
use crate::initiatives::channel_names;

/// One fleet on a network, as the navigator groups it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FleetPlace {
    /// The fleet's id in the API, which keys its navigator group.
    pub id: String,
    /// Its name in the API, else its slug, else its id.
    pub name: String,
    /// Its slug, when the API gives one: the prefix its channels drop.
    pub slug: Option<String>,
    /// Its channels in lower case: the fleet's own, then its initiatives'.
    channels: HashSet<String>,
    legacy: HashMap<String, String>,
}

impl FleetPlace {
    /// Whether `channel` is one of this fleet's, ignoring ASCII case.
    pub(crate) fn holds(&self, channel: &str) -> bool {
        self.channels.contains(&channel.to_ascii_lowercase())
    }

    /// The current channel corresponding to a former installer name.
    pub(crate) fn canonical(&self, channel: &str) -> Option<&str> {
        self.legacy
            .get(&channel.to_ascii_lowercase())
            .map(String::as_str)
    }
}

/// `channel` as its fleet's group shows it: without the leading `#` and
/// without the fleet's `<slug>-` prefix, so `#aurora-lyra` reads `lyra`. A
/// name that would be left empty keeps its prefix.
pub(crate) fn short_name<'a>(slug: Option<&str>, channel: &'a str) -> &'a str {
    let bare = channel.strip_prefix('#').unwrap_or(channel);
    let rest = slug.and_then(|slug| {
        let head = bare.get(..slug.len())?;
        let rest = bare.get(slug.len()..)?.strip_prefix('-')?;
        (head.eq_ignore_ascii_case(slug) && !rest.is_empty()).then_some(rest)
    });
    rest.unwrap_or(if bare.is_empty() { channel } else { bare })
}

/// The fleets of `initiatives`, each once by its id in their first order,
/// with the channels each claims alone.
pub(super) fn places(initiatives: &[&MemberInitiative]) -> Vec<FleetPlace> {
    let mut order: Vec<&Fleet> = Vec::new();
    let mut claimed: HashMap<String, Vec<String>> = HashMap::new();
    for initiative in initiatives {
        let Some(fleet) = initiative.fleet.as_ref() else {
            continue;
        };
        if !order.iter().any(|held| held.id == fleet.id) {
            order.push(fleet);
        }
        let own = Names::of(fleet).channels.into_iter().map(|(name, _)| name);
        let named = channel_names(&initiative.channels);
        claimed
            .entry(fleet.id.clone())
            .or_default()
            .extend(own.chain(named).map(|name| name.to_ascii_lowercase()));
    }
    let shared = shared(&claimed);
    order
        .into_iter()
        .map(|fleet| FleetPlace {
            id: fleet.id.clone(),
            name: display_name(fleet),
            legacy: legacy_channels(fleet, &claimed),
            slug: Some(fleet.slug.clone()).filter(|slug| !slug.is_empty()),
            channels: claimed
                .get(&fleet.id)
                .into_iter()
                .flatten()
                .cloned()
                .filter(|name| !shared.contains(name))
                .collect(),
        })
        .collect()
}

/// Former installer names that are not still claimed as real channels.
fn legacy_channels(
    fleet: &Fleet,
    claimed: &HashMap<String, Vec<String>>,
) -> HashMap<String, String> {
    let slug = fleet.slug.to_ascii_lowercase();
    if slug.is_empty() {
        return HashMap::new();
    }
    Names::of(fleet)
        .channels
        .into_iter()
        .filter_map(|(channel, kind)| {
            let channel = channel.to_ascii_lowercase();
            let former = match kind {
                ChannelKind::Audit => format!("#{slug}-fleet-audit"),
                ChannelKind::Ship { .. } => format!("#{slug}-{}", channel.strip_prefix('#')?),
                _ => return None,
            };
            let prefixed = channel.starts_with(&format!("#{slug}-"));
            let taken = claimed.values().flatten().any(|name| name == &former);
            (!(taken || prefixed && kind != ChannelKind::Audit)).then_some((former, channel))
        })
        .collect()
}

/// The channels that more than one fleet claims.
fn shared(claimed: &HashMap<String, Vec<String>>) -> HashSet<String> {
    let mut claims: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (fleet, names) in claimed {
        for name in names {
            claims.entry(name).or_default().insert(fleet);
        }
    }
    claims
        .into_iter()
        .filter(|(_, fleets)| fleets.len() > 1)
        .map(|(name, _)| name.to_owned())
        .collect()
}

/// A fleet's name as the API gives it, else its slug, else its id.
fn display_name(fleet: &Fleet) -> String {
    [&fleet.name, &fleet.slug, &fleet.id]
        .into_iter()
        .map(|name| name.trim())
        .find(|name| !name.is_empty())
        .unwrap_or_default()
        .to_owned()
}
