use std::{hint::black_box, mem::size_of, time::Instant};

use gpui_kit::{SharedString, base::TextView};

use super::{
    Cache, Key, MAX_ENTRIES, MAX_PREFLIGHT_BYTES, kit_options, needs_preflight, parses, prepare,
};

#[test]
fn parser_panics_return_errors_and_small_crashes_remain_readable() {
    for source in [
        "1. ```\n- x\n",
        "1. a\n   ```\n- x\n",
        "1. a\n   ```\n- x\n```",
    ] {
        let parsed = std::panic::catch_unwind(|| markdown::to_mdast(source, &kit_options()));
        let error = parsed
            .expect("the public parser must not unwind")
            .expect_err("regression input");
        assert_eq!(error.reason, "The Markdown parser failed on this text.");
        let checked = prepare(source.to_owned().into());
        assert_eq!(checked.as_ref(), crate::rich::escape_markdown(source));
        assert!(parses(&checked));
    }
}

#[test]
fn only_small_fence_or_math_sources_are_preflighted_without_copying_valid_text() {
    for source in ["```code", "~~~code", "$$math"] {
        assert!(needs_preflight(source));
    }
    assert!(!needs_preflight("**normal** `code`"));
    assert!(needs_preflight(&format!(
        "```{}",
        "a".repeat(MAX_PREFLIGHT_BYTES - 3)
    )));
    let large: SharedString = format!("```{}", "a".repeat(MAX_PREFLIGHT_BYTES)).into();
    assert!(!needs_preflight(&large));
    // Short SharedStrings use inline storage, whose address changes on a move.
    let original: SharedString = format!("```\n{}\n```", "safe".repeat(32)).into();
    assert_eq!(prepare(original.clone()).as_ptr(), original.as_ptr());
    assert_eq!(prepare(large.clone()).as_ptr(), large.as_ptr());
}

#[test]
fn verdict_cache_is_lru_bounded_and_retains_no_source_text() {
    let mut cache = Cache::default();
    for index in 0..MAX_ENTRIES {
        cache.insert(
            Key {
                hash: index as u64,
                len: 10,
            },
            true,
        );
    }
    let first = Key { hash: 0, len: 10 };
    assert_eq!(cache.get(first), Some(true));
    cache.insert(
        Key {
            hash: MAX_ENTRIES as u64,
            len: 10,
        },
        false,
    );
    assert_eq!(cache.get(Key { hash: 1, len: 10 }), None);
    assert_eq!(cache.get(first), Some(true));
    assert_eq!(cache.get(Key { hash: 0, len: 11 }), None);
    assert_eq!(cache.entries.len(), MAX_ENTRIES);
    // The complete entry type is Copy numeric metadata plus a verdict: no
    // String, SharedString, reference, source bytes or escaped payload exists.
    let metadata: Vec<(Key, bool)> = cache.entries.iter().copied().collect();
    assert_eq!(
        size_of::<Cache>(),
        size_of::<std::collections::VecDeque<(Key, bool)>>()
    );
    assert_eq!(metadata.len(), MAX_ENTRIES);
}

#[test]
fn measure_transcript_streaming_and_large_source_cost() {
    let transcript: Vec<SharedString> = (0..2_000).map(|index| format!(
        "**Report {index}:** `cargo test` passed. See [details](https://example.test/{index})\n- next step\n- ready"
    ).into()).collect();
    let visible = &transcript[transcript.len() - 200..];
    let cold = elapsed(|| {
        for source in visible {
            black_box(prepare(source.clone()));
        }
    });
    let warm = elapsed(|| {
        for _ in 0..60 {
            for source in visible {
                black_box(prepare(source.clone()));
            }
        }
    });
    let fenced: Vec<SharedString> = visible
        .iter()
        .map(|text| format!("{text}\n```\ncode\n```").into())
        .collect();
    let fence_cold = elapsed(|| {
        for source in &fenced {
            black_box(prepare(source.clone()));
        }
    });
    let fence_warm = elapsed(|| {
        for _ in 0..60 {
            for source in &fenced {
                black_box(prepare(source.clone()));
            }
        }
    });
    eprintln!(
        "Guard: 2,000 messages / 200 visible: cold={cold:?}, warm/frame={:?}; fenced cold={fence_cold:?}, fenced warm/frame={:?}",
        warm / 60,
        fence_warm / 60
    );
    measure_stream_and_large();
    assert!(visible.iter().all(|source| !needs_preflight(source)));
}

fn measure_stream_and_large() {
    let reply = format!("```rust\n{}", "let x = 1;\n".repeat(2_000));
    let chunks: Vec<_> = (1..=200).map(|index| &reply[..index * 100]).collect();
    let kit = elapsed(|| {
        for source in &chunks {
            black_box(TextView::markdown("bench", (*source).to_owned()));
        }
    });
    let existing_preparation = elapsed(|| {
        for source in &chunks {
            black_box(TextView::markdown(
                "bench",
                crate::rich::prepare_markdown(source),
            ));
        }
    });
    let rich = elapsed(|| {
        for source in &chunks {
            black_box(crate::rich::rich_text(
                "bench",
                crate::rich::Format::Markdown,
                source,
            ));
        }
    });
    let large: SharedString = format!("```\n{}", "a".repeat(300 * 1024)).into();
    let large_guard = elapsed(|| {
        for _ in 0..200 {
            black_box(prepare(large.clone()));
        }
    });
    let large_rich = elapsed(|| {
        black_box(crate::rich::rich_text(
            "bench",
            crate::rich::Format::Markdown,
            &large,
        ));
    });
    eprintln!(
        "20 KB / 200 chunks: Kit constructor={kit:?}, Kit + existing preparation={existing_preparation:?}, rich_text={rich:?}; 300 KiB: guard/call={:?}, rich_text={large_rich:?}",
        large_guard / 200
    );
}

fn elapsed(action: impl FnOnce()) -> std::time::Duration {
    let start = Instant::now();
    action();
    start.elapsed()
}
