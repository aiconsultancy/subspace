use chrono::Utc;

use super::ThreadRoots;
use crate::{BufferKey, Line, LineKind, NetworkId, Target};

#[test]
fn anonymous_replies_share_cached_ancestors_including_notices_and_actions() {
    let buffer = BufferKey {
        network: NetworkId("fixture".into()),
        target: Target::Channel("#threads".into()),
    };
    let loaded: Vec<Line> = (0..5_000)
        .map(|index| {
            let mut line = Line::new(buffer.clone(), Utc::now(), LineKind::Privmsg, String::new());
            line.id = Some(index.to_string());
            line.reply_to = (index > 0).then(|| (index - 1).to_string());
            line
        })
        .collect();
    let anonymous: Vec<Line> = (0..5_000)
        .map(|index| {
            let kind = [LineKind::Privmsg, LineKind::Notice, LineKind::Action][index % 3];
            let mut line = Line::new(buffer.clone(), Utc::now(), kind, String::new());
            line.reply_to = Some("4999".into());
            line
        })
        .collect();
    let roots = ThreadRoots::new(&buffer, loaded.iter().chain(&anonymous));
    for line in &anonymous {
        assert_eq!(roots.root(line), Some("0"));
    }
    assert_eq!(
        roots.walked.get(),
        loaded.len(),
        "ID-less replies must populate and reuse the ancestor cache"
    );
}

#[test]
fn nested_reply_aggregation_walks_each_ancestor_once_in_either_order() {
    let buffer = BufferKey {
        network: NetworkId("fixture".into()),
        target: Target::Channel("#threads".into()),
    };
    let loaded: Vec<Line> = (0..10_000)
        .map(|index| {
            let mut line = Line::new(buffer.clone(), Utc::now(), LineKind::Privmsg, String::new());
            line.id = Some(index.to_string());
            line.reply_to = (index > 0).then(|| (index - 1).to_string());
            line
        })
        .collect();
    for reverse in [false, true] {
        let roots = ThreadRoots::new(&buffer, &loaded);
        let mut order: Vec<&Line> = loaded.iter().collect();
        if reverse {
            order.reverse();
        }
        for line in order {
            assert_eq!(roots.root(line), Some("0"));
        }
        assert_eq!(
            roots.walked.get(),
            loaded.len(),
            "cached ancestors are not walked again"
        );
    }
}

#[test]
fn cached_cycles_stay_invalid_and_unindexed_lines_cannot_poison_the_index() {
    let buffer = BufferKey {
        network: NetworkId("fixture".into()),
        target: Target::Query("astra".into()),
    };
    let mut root = Line::new(buffer.clone(), Utc::now(), LineKind::Privmsg, String::new());
    root.id = Some("root".into());
    let mut child = root.clone();
    child.id = Some("child".into());
    child.reply_to = Some("root".into());
    let mut first = root.clone();
    first.id = Some("first".into());
    first.reply_to = Some("second".into());
    let mut second = first.clone();
    second.id = Some("second".into());
    second.reply_to = Some("first".into());
    let roots = ThreadRoots::new(&buffer, [&root, &child, &first, &second]);
    let mut impostor = root.clone();
    impostor.reply_to = Some("child".into());
    assert_eq!(roots.root(&impostor), None);
    assert_eq!(roots.root(&child), Some("root"));
    assert_eq!(roots.root(&first), None);
    let before = roots.walked.get();
    assert_eq!(roots.root(&second), None);
    assert_eq!(roots.walked.get(), before, "invalid chains are cached too");
}
