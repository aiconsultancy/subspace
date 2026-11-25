//! An item's pages joined into the whole item within a limit, or read one
//! part at a time pinned to their version, and checked against the item
//! they name.

use super::*;

fn page(seq: u64, page: u32, pages: u32, data: &str, bytes: u64) -> ItemPage {
    ItemPage {
        agent: AGENT.into(),
        turn: "t-now".into(),
        item: "i3".into(),
        seq,
        page,
        pages,
        bytes,
        data: data.into(),
    }
}

/// The JSON a join gave whole.
fn whole_json(joined: Option<Result<Joined, Error>>) -> String {
    match joined {
        Some(Ok(Joined::Whole { json, .. })) => json,
        _ => panic!("the whole item"),
    }
}

#[test]
fn pages_join_into_the_whole_item_and_restart_when_it_changes() {
    let whole = r#"{"id":"i3"}"#;
    let (a, b) = whole.split_at(5);
    let reads = Mutex::new(Vec::new());
    // Version 1 changes to version 2 after page 0 was read.
    let joined = join_pages(&AtomicBool::new(false), u64::MAX, |number| {
        let mut reads = reads.lock().unwrap();
        reads.push(number);
        let version = if reads.len() == 1 { 1 } else { 2 };
        let data = if number == 0 { a } else { b };
        Ok(page(version, number, 2, data, whole.len() as u64))
    });
    assert!(matches!(&joined, Some(Ok(Joined::Whole { seq: 2, .. }))));
    assert_eq!(whole_json(joined), whole);
    assert_eq!(*reads.lock().unwrap(), [0, 1, 0, 1]);
}

#[test]
fn a_join_never_cuts_and_gives_up_on_an_item_that_keeps_changing() {
    let go = AtomicBool::new(false);
    let short = join_pages(&go, u64::MAX, |number| Ok(page(1, number, 1, "{}", 9)));
    assert!(matches!(short, Some(Err(Error::InvalidResponse))));
    let mut version = 0;
    let busy = join_pages(&go, u64::MAX, |number| {
        version += 1;
        Ok(page(version, number, 2, "{", 2))
    });
    assert!(matches!(busy, Some(Err(Error::Busy(None)))));
}

/// SUB decision `a17870f4`: no item is too large to read. One larger than
/// the reader can hold gives page 0 alone, before any other page is read.
#[test]
fn an_item_over_the_limit_gives_page_0_alone_and_none_is_too_large() {
    let go = AtomicBool::new(false);
    let reads = Mutex::new(0);
    let huge = join_pages(&go, 64 * 1024 * 1024, |number| {
        *reads.lock().unwrap() += 1;
        Ok(page(1, number, 1_000_000, "{", 60_000_000_000))
    });
    let Some(Ok(Joined::Larger(first))) = huge else {
        panic!("page 0 of an item to read in parts");
    };
    assert_eq!((first.page, first.bytes), (0, 60_000_000_000));
    assert_eq!(*reads.lock().unwrap(), 1, "no page after page 0");
    // Within the limit, the same size is read whole.
    let small = r#"{"id":"i3"}"#;
    let within = join_pages(&go, small.len() as u64, |number| {
        Ok(page(1, number, 1, small, small.len() as u64))
    });
    assert_eq!(whole_json(within), small);
}

#[test]
fn a_page_0_that_declares_an_impossible_size_is_refused_before_any_other_read() {
    let go = AtomicBool::new(false);
    // No pages, more bytes than the pages hold, more pages than bytes, a
    // page past the declared size, or one with nothing in it.
    let refused = [
        vec![page(1, 0, 0, "", 0)],
        vec![page(1, 0, 2, "{}", 2 * 64 * 1024 + 1)],
        vec![page(1, 0, 5, "{", 2)],
        vec![page(1, 0, 2, "{", 2), page(1, 1, 2, "}}", 2)],
        vec![page(1, 0, 3, "{", 3), page(1, 1, 3, "", 3)],
    ];
    for pages in refused {
        let joined = join_pages(&go, u64::MAX, |number| {
            Ok(pages
                .get(number as usize)
                .cloned()
                .unwrap_or_else(|| page(1, number, 1, "", 0)))
        });
        assert!(matches!(joined, Some(Err(Error::InvalidResponse))));
    }
}

/// Captain's ruling on round 3, point 5: a whole read never holds more
/// than the room made for it. Page 0 claiming more comes back alone, and
/// pages that together pass the size page 0 claimed are refused.
#[test]
fn a_whole_read_never_holds_more_than_its_limit() {
    let go = AtomicBool::new(false);
    let reads = Mutex::new(0);
    let over = join_pages(&go, 3, |number| {
        *reads.lock().unwrap() += 1;
        Ok(page(1, number, 2, "{\"", 4))
    });
    assert!(matches!(over, Some(Ok(Joined::Larger(_)))));
    assert_eq!(*reads.lock().unwrap(), 1, "no page after page 0");
    let past = join_pages(&go, 3, |number| {
        let data = if number == 0 { "{" } else { "}}}" };
        Ok(page(1, number, 2, data, 3))
    });
    assert!(matches!(past, Some(Err(Error::InvalidResponse))));
    let pages_over_bytes = join_pages(&go, 3, |number| Ok(page(1, number, 4, "{", 3)));
    assert!(matches!(
        pages_over_bytes,
        Some(Err(Error::InvalidResponse))
    ));
}

#[test]
fn a_cancelled_join_reads_no_page_after_the_flag_is_set() {
    let cancel = AtomicBool::new(false);
    let reads = Mutex::new(Vec::new());
    let joined = join_pages(&cancel, u64::MAX, |number| {
        reads.lock().unwrap().push(number);
        // The view is closed while page 0 is being read.
        cancel.store(true, Ordering::Release);
        Ok(page(1, number, 3, "{", 3))
    });
    assert!(joined.is_none());
    assert_eq!(*reads.lock().unwrap(), [0]);
    assert!(join_pages(&cancel, u64::MAX, |_| unreachable!("never read")).is_none());
}

/// GPT review P1 at `9c53ac21`: a paged item of a kind this version does
/// not know, or of a known kind in a shape it does not know, is kept as
/// its JSON, as an inline one is, down to its last bytes.
#[test]
fn a_paged_item_of_an_unknown_kind_or_shape_keeps_its_json() {
    let unknown = json!({"id": "i3", "seq": 6, "startedAt": 1, "updatedAt": 2,
                         "kind": "patch", "body": "UNKNOWN-KIND-END"})
    .to_string();
    let ActivityEntry::Unknown(item) = decode_item(&unknown, "i3", 6).ok().unwrap() else {
        panic!("an item of an unknown kind");
    };
    assert_eq!(item.kind, "patch");
    assert_eq!(item.raw, serde_json::from_str::<Value>(&unknown).unwrap());
    let reshaped = json!({"id": "i3", "seq": 6, "startedAt": 1, "updatedAt": 2,
                          "kind": "tool", "status": 7, "tail": "CHANGED-SHAPE-END"})
    .to_string();
    let ActivityEntry::Unknown(item) = decode_item(&reshaped, "i3", 6).ok().unwrap() else {
        panic!("a tool item of an unknown shape");
    };
    assert_eq!(item.raw["tail"], json!("CHANGED-SHAPE-END"));
    let known = text("i3", 6, "known").to_string();
    assert!(matches!(
        decode_item(&known, "i3", 6),
        Ok(ActivityEntry::Item(_))
    ));
    assert!(
        decode_item("[1, 2]", "i3", 6).is_err(),
        "not an item at all"
    );
}

/// GPT review P2 at `9c53ac21`: pages whose envelopes are sound but whose
/// joined item names another ID or `seq` are an invalid answer.
#[test]
fn a_joined_item_must_be_the_one_its_pages_name() {
    let other_id = text("i4", 6, "elsewhere").to_string();
    assert!(matches!(
        decode_item(&other_id, "i3", 6),
        Err(Error::InvalidResponse)
    ));
    let other_seq = text("i3", 5, "older").to_string();
    assert!(matches!(
        decode_item(&other_seq, "i3", 6),
        Err(Error::InvalidResponse)
    ));
    let unknown_elsewhere = json!({"id": "i4", "seq": 6, "startedAt": 1, "updatedAt": 2,
                                   "kind": "patch"})
    .to_string();
    assert!(decode_item(&unknown_elsewhere, "i3", 6).is_err());
}

/// A part of an item too large to hold whole is read pinned to the
/// version it began with; once the item changed, page 0 of the newest
/// version comes instead, read once.
#[test]
fn a_part_is_pinned_to_its_version_and_restarts_once_when_it_changed() {
    let go = AtomicBool::new(false);
    let first = page(6, 0, 3, "{\"a\":", 9);
    let part = read_part(&go, 2, Some(&first), |number| {
        Ok(page(6, number, 3, "1}", 9))
    })
    .unwrap()
    .ok()
    .unwrap();
    assert_eq!((part.page.page, part.newer), (2, false));
    let reads = Mutex::new(Vec::new());
    let part = read_part(&go, 2, Some(&first), |number| {
        reads.lock().unwrap().push(number);
        Ok(page(7, number, 3, "{\"b\"", 10))
    })
    .unwrap()
    .ok()
    .unwrap();
    assert_eq!((part.page.page, part.page.seq, part.newer), (0, 7, true));
    assert_eq!(
        *reads.lock().unwrap(),
        [2, 0],
        "read again once, from page 0"
    );
    let unsound = read_part(&go, 1, Some(&first), |number| Ok(page(6, number, 3, "", 9)));
    assert!(matches!(unsound, Some(Err(Error::InvalidResponse))));
    let cancel = AtomicBool::new(true);
    assert!(read_part(&cancel, 1, Some(&first), |_| unreachable!("never read")).is_none());
}
