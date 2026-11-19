use super::DraftLimits;
use crate::badge::PillTone;

#[test]
fn line_counts_reveal_after_one_line_and_warn_at_eighty_percent() {
    for (limit, lines, expected) in [
        (100, 1, None),
        (100, 2, Some(PillTone::Muted)),
        (100, 79, Some(PillTone::Muted)),
        (100, 80, Some(PillTone::Warning)),
        (100, 100, Some(PillTone::Warning)),
        (100, 101, Some(PillTone::Danger)),
        (10, 7, Some(PillTone::Muted)),
        (10, 8, Some(PillTone::Warning)),
        (10, 10, Some(PillTone::Warning)),
        (10, 11, Some(PillTone::Danger)),
    ] {
        let text = vec!["line"; lines].join("\n");
        let counter = DraftLimits::new(limit, 4096).counter(&text);
        assert_eq!(
            counter.as_ref().map(|value| value.tone),
            expected,
            "{lines}/{limit}"
        );
        if let Some(counter) = counter {
            assert_eq!(counter.text, format!("{lines} / {limit} lines"));
        }
    }
}

#[test]
fn bytes_reveal_strictly_above_half_and_use_utf8_sizes() {
    for (bytes, expected) in [
        (2048, None),
        (2049, Some(("2.0 / 4 KB", PillTone::Muted))),
        (3235, Some(("3.2 / 4 KB", PillTone::Muted))),
        (3276, Some(("3.2 / 4 KB", PillTone::Muted))),
        (3277, Some(("3.2 / 4 KB", PillTone::Warning))),
        (4096, Some(("4.0 / 4 KB", PillTone::Warning))),
        (4097, Some(("4.0 / 4 KB", PillTone::Danger))),
    ] {
        let counter = DraftLimits::new(100, 4096).counter(&"a".repeat(bytes));
        assert_eq!(
            counter
                .as_ref()
                .map(|value| (value.text.as_str(), value.tone)),
            expected,
            "{bytes}"
        );
    }
    let unicode = DraftLimits::new(10, 4096)
        .counter(&"é".repeat(2048))
        .unwrap();
    assert_eq!(unicode.text, "4.0 / 4 KB");
    assert_eq!(unicode.label, "Draft limits: 4.0 / 4 KB. Near the limit.");
}

#[test]
fn both_counts_share_one_pill_and_trailing_newlines_follow_validation() {
    let limits = DraftLimits::new(100, 4096);
    let draft = format!("{}{}", "a\n".repeat(41), "a".repeat(2273));
    let counter = limits.counter(&draft).unwrap();
    assert_eq!(counter.text, "42 / 100 lines · 2.3 / 4 KB");
    assert_eq!(counter.tone, PillTone::Muted);
    assert_eq!(limits.counter("one\n"), None);
    assert_eq!(limits.counter(""), None);
}

#[test]
fn labels_change_only_with_displayed_counts_or_tone() {
    let limits = DraftLimits::new(100, 4096);
    let counter = |bytes| limits.counter(&"a".repeat(bytes)).unwrap();
    assert_eq!(counter(2049).label, counter(2050).label);
    assert_eq!(counter(2099).label, counter(2049).label);
    assert_ne!(counter(2099).label, counter(2100).label);
    assert_eq!(counter(3276).text, counter(3277).text);
    assert_ne!(counter(3276).label, counter(3277).label);
    assert!(counter(3277).label.ends_with("Near the limit."));
    assert_eq!(counter(4096).text, counter(4097).text);
    assert_ne!(counter(4096).label, counter(4097).label);
    assert!(counter(4097).label.ends_with("Over the limit."));
    assert_eq!(limits.counter("one\ntwo"), limits.counter("one\ntwo more"));
    assert_ne!(
        limits.counter("one\ntwo"),
        limits.counter("one\ntwo\nthree")
    );
}

#[test]
fn zero_limits_are_clamped_to_one() {
    assert_eq!(DraftLimits::new(0, 0), DraftLimits::new(1, 1));
}
