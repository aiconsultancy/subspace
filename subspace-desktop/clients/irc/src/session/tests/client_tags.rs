//! Client-only tags kept on lines, live and from history, so a Space can
//! read a machine-readable marker another client added.

use super::{ALL_CAPS, feed, in_channel, lines};
use crate::model::{Event, Line};

/// What a live channel feed carries: a tagged notice, then a multiline
/// message whose opening batch and inner lines carry different tags.
const LIVE: &[&str] = &[
    "@msgid=n;account=astra;+draft/reply=root;+example.org/progress=tool :astra!a@h NOTICE #c :progress",
    "@msgid=ml;+example.org/progress=completed :astra!a@h BATCH +m draft/multiline #c",
    "@batch=m;+example.org/progress=wrong;+example.org/extra=1 :astra!a@h PRIVMSG #c :one",
    "@batch=m :astra!a@h PRIVMSG #c :two",
    ":astra!a@h BATCH -m",
    ":bob!b@h PRIVMSG #c :untagged",
];

/// A history page holding one tagged notice.
const HISTORY: &[&str] = &[
    ":ergo.test BATCH +h draft/chathistory #c",
    "@batch=h;msgid=old;+draft/reply=root;+example.org/progress=waiting :astra!a@h NOTICE #c :waiting",
    ":ergo.test BATCH -h",
];

fn client_tags(line: &Line) -> Vec<String> {
    line.client_tags
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect()
}

fn assert_live(held: &[&Line]) {
    assert_eq!(
        client_tags(held[0]),
        ["+example.org/progress=tool"],
        "server tags and the reply reference stay out"
    );
    assert_eq!(held[0].reply_to.as_deref(), Some("root"));
    assert_eq!(held[1].text, "one\ntwo");
    assert_eq!(
        client_tags(held[1]),
        ["+example.org/progress=completed"],
        "a multiline message takes the opening batch's tags alone"
    );
    assert!(held[2].client_tags.is_empty());
}

#[test]
fn client_only_tags_are_kept_live_in_history_and_from_the_opening_batch_only() {
    let mut session = in_channel(ALL_CAPS);
    assert_live(&lines(&feed(&mut session, LIVE)));
    let history = feed(&mut session, HISTORY);
    let page = history.events.iter().find_map(|event| match event {
        Event::History { lines, .. } => Some(lines),
        _ => None,
    });
    let tags = page.and_then(|page| page.first()).map(client_tags);
    assert_eq!(tags, Some(vec!["+example.org/progress=waiting".to_owned()]));
}
