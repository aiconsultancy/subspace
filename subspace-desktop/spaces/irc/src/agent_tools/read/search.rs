//! `search`: messages that contain a query, in the lines the board holds
//! and the messages it keeps in the store, newest first, each once.

use chrono::SecondsFormat;
use serde_json::{Value, json};
use subspace_client_irc::{BufferKey, Target, format as irc_format};
use subspace_framework::dashboard::full_text::{Filters, Hit, Query, Ranking, Service};

use super::super::lookup;
use super::messages::line;
use crate::state::{Entry, IrcState, is_message};
use crate::store;

pub(super) fn search(
    state: &IrcState,
    query: &str,
    network: Option<&str>,
    limit: usize,
) -> Result<Value, String> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Err("Search needs a query.".into());
    }
    let only = network
        .map(|id| lookup::network(state, id).map(|network| network.id.clone()))
        .transpose()?;
    let mut matches: Vec<(&BufferKey, &Entry)> = state
        .buffers()
        .filter(|(key, _)| only.as_ref().is_none_or(|id| &key.network == id))
        .flat_map(|(key, buffer)| buffer.entries.iter().map(move |entry| (key, entry)))
        .filter(|(_, entry)| {
            is_message(entry.line.kind)
                && irc_format::strip(&entry.line.text)
                    .to_lowercase()
                    .contains(&needle)
        })
        .collect();
    matches.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.line.time));
    let truncated = matches.len() > limit;
    let found: Vec<Value> = matches
        .iter()
        .take(limit)
        .map(|(key, entry)| {
            let mut value = line(entry);
            value["network"] = json!(key.network.0);
            value["target"] = json!(lookup::target_name(&key.target));
            value["id"] = json!(store::document_id(&entry.line));
            value
        })
        .collect();
    Ok(json!({
        "matches": found,
        "truncated": truncated,
        "network": only.map(|id| id.0),
    }))
}

/// `held`, the hub's answer, with the store's matches for `query` that it
/// does not hold, newest first, at most `limit`. A store that is off or
/// fails adds nothing.
pub(super) fn with_stored(mut held: Value, service: &Service, query: &str, limit: usize) -> Value {
    let filters = Filters {
        project: held["network"].as_str().map(str::to_owned),
        ..Default::default()
    };
    let request = Query {
        text: query.to_owned(),
        boards: vec![crate::ID.to_owned()],
        limit,
        ranking: Ranking::Bm25,
    };
    let (hits, more) = service
        .search_filtered(&request, &filters)
        .map(|response| (response.hits, response.truncated))
        .unwrap_or_default();
    let matches = merge(held["matches"].take(), &hits, limit);
    let truncated = held["truncated"].as_bool().unwrap_or_default() || more || matches.1;
    held["matches"] = Value::Array(matches.0);
    held["truncated"] = json!(truncated);
    held
}

/// The held matches and the stored hits they do not hold, newest first,
/// cut to `limit`; true when some were cut.
fn merge(held: Value, hits: &[Hit], limit: usize) -> (Vec<Value>, bool) {
    let mut matches: Vec<Value> = match held {
        Value::Array(matches) => matches,
        _ => Vec::new(),
    };
    for hit in hits {
        let known = matches
            .iter()
            .any(|found| found["id"].as_str() == Some(hit.document.id.as_str()));
        if let (false, Some(found)) = (known, stored(hit)) {
            matches.push(found);
        }
    }
    matches.sort_by(|left, right| right["time"].as_str().cmp(&left["time"].as_str()));
    let truncated = matches.len() > limit;
    matches.truncate(limit);
    (matches, truncated)
}

/// A stored hit in the shape of a held match, marked as stored.
fn stored(hit: &Hit) -> Option<Value> {
    let navigation = store::navigation(&hit.document)?;
    let target = match &navigation.target {
        Target::Channel(name) | Target::Query(name) => name.clone(),
        Target::Server => return None,
    };
    Some(json!({
        "id": hit.document.id,
        "time": navigation.time.to_rfc3339_opts(SecondsFormat::Secs, true),
        "kind": "message",
        "from": navigation.from,
        "text": hit.document.body,
        "network": navigation.network.0,
        "target": target,
        "stored": true,
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::with_stored;
    use crate::fixtures::{channel, message};
    use crate::store::tests::{recent, service};
    use crate::store::{self, Link, Op};

    #[test]
    fn stored_messages_join_the_held_matches_once_each_newest_first() {
        let (_root, service) = service();
        let room = channel("#subspace");
        let held_line = recent(message(&room, "alice", "rocket ready?", 5));
        let older = recent(message(&room, "bob", "the rocket manual", 1));
        let other = recent(message(
            &crate::fixtures::query("carol"),
            "carol",
            "nothing here",
            2,
        ));
        let errors = Link::new(service.clone()).run(
            vec![Op::Publish(vec![held_line.clone(), older.clone(), other])],
            chrono::Utc::now(),
        );
        assert!(errors.is_empty());
        let held = json!({
            "matches": [{
                "id": store::document_id(&held_line),
                "time": held_line.time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                "text": "rocket ready?",
            }],
            "truncated": false,
            "network": "local",
        });
        let answer = with_stored(held, &service, "rocket", 10);
        let matches = answer["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0]["text"], "rocket ready?");
        assert_eq!(matches[1]["text"], "the rocket manual");
        assert_eq!(matches[1]["stored"], true);
        assert_eq!(matches[1]["from"], "bob");
        assert_eq!(matches[1]["target"], "#subspace");
        let cut = with_stored(
            json!({"matches": [], "truncated": false, "network": null}),
            &service,
            "rocket",
            1,
        );
        assert_eq!(cut["truncated"], true);
    }
}
