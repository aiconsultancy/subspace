//! Client-only tags kept on lines, live and from history, so a Space can
//! read the progress marker an agent gateway adds. Only the allow-listed
//! tags with short values are kept.

use super::{ALL_CAPS, feed, in_channel, lines};
use crate::model::{Event, Line};

/// What a live channel feed carries: a tagged notice with a tag outside
/// the allow-list, a multiline message whose opening batch and inner lines
/// carry different tags, an untagged message and a notice whose value is
/// too long.
const LIVE: &[&str] = &[
    "@msgid=n;account=astra;+draft/reply=root;+stargate/progress=tool;+example.org/other=1 :astra!a@h NOTICE #c :progress",
    "@msgid=ml;+stargate/progress=completed :astra!a@h BATCH +m draft/multiline #c",
    "@batch=m;+stargate/progress=wrong;+example.org/extra=1 :astra!a@h PRIVMSG #c :one",
    "@batch=m :astra!a@h PRIVMSG #c :two",
    ":astra!a@h BATCH -m",
    ":bob!b@h PRIVMSG #c :untagged",
    "@msgid=long;+draft/reply=root;+stargate/progress=xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx :astra!a@h NOTICE #c :long",
];

/// A history page holding one tagged notice.
const HISTORY: &[&str] = &[
    ":ergo.test BATCH +h draft/chathistory #c",
    "@batch=h;msgid=old;+draft/reply=root;+stargate/progress=waiting;+example.org/other=2 :astra!a@h NOTICE #c :waiting",
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
        ["+stargate/progress=tool"],
        "server tags, the reply reference and other client-only tags stay out"
    );
    assert_eq!(held[0].reply_to.as_deref(), Some("root"));
    assert_eq!(held[1].text, "one\ntwo");
    assert_eq!(
        client_tags(held[1]),
        ["+stargate/progress=completed"],
        "a multiline message takes the opening batch's tags alone"
    );
    assert!(held[2].client_tags.is_empty());
    assert!(
        held[3].client_tags.is_empty(),
        "a value longer than the cap is dropped"
    );
    assert_eq!(held[3].reply_to.as_deref(), Some("root"));
}

#[test]
fn only_allowed_client_tags_are_kept_live_in_history_and_from_the_opening_batch() {
    let mut session = in_channel(ALL_CAPS);
    assert_live(&lines(&feed(&mut session, LIVE)));
    let history = feed(&mut session, HISTORY);
    let page = history.events.iter().find_map(|event| match event {
        Event::History { lines, .. } => Some(lines),
        _ => None,
    });
    let tags = page.and_then(|page| page.first()).map(client_tags);
    assert_eq!(tags, Some(vec!["+stargate/progress=waiting".to_owned()]));
}
