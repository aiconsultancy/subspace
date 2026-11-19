use super::{Span, parse, strip};

fn span(text: &str) -> Span {
    Span {
        text: text.to_string(),
        ..Span::default()
    }
}

#[test]
fn plain_text_is_one_span() {
    assert_eq!(parse("hello world"), vec![span("hello world")]);
    assert!(parse("").is_empty());
}

#[test]
fn toggles_and_reset() {
    let spans = parse("a\u{02}b\u{1d}c\u{02}d\u{0f}e\u{1f}\u{1e}\u{11}f");
    assert_eq!(spans.len(), 6);
    assert_eq!(spans[0], span("a"));
    assert_eq!(
        spans[1],
        Span {
            bold: true,
            ..span("b")
        }
    );
    assert_eq!(
        spans[2],
        Span {
            bold: true,
            italic: true,
            ..span("c")
        }
    );
    assert_eq!(
        spans[3],
        Span {
            italic: true,
            ..span("d")
        }
    );
    assert_eq!(spans[4], span("e"));
    assert_eq!(
        spans[5],
        Span {
            underline: true,
            strikethrough: true,
            monospace: true,
            ..span("f")
        }
    );
}

#[test]
fn colours() {
    let spans = parse("\u{03}4red\u{03}12,01blue\u{03}plain");
    assert_eq!(
        spans,
        vec![
            Span {
                fg: Some(4),
                ..span("red")
            },
            Span {
                fg: Some(12),
                bg: Some(1),
                ..span("blue")
            },
            span("plain"),
        ]
    );
}

#[test]
fn colour_edge_cases() {
    // A comma without a digit stays text; three digits keep the third.
    assert_eq!(
        parse("\u{03}4,x"),
        vec![Span {
            fg: Some(4),
            ..span(",x")
        }]
    );
    assert_eq!(
        parse("\u{03}045"),
        vec![Span {
            fg: Some(4),
            ..span("5")
        }]
    );
    // 99 is the default colour.
    assert_eq!(parse("\u{03}99,99x"), vec![span("x")]);
}

#[test]
fn hex_colours_map_to_default() {
    assert_eq!(parse("\u{04}FF0000,00FF00red"), vec![span("red")]);
}

#[test]
fn reverse_swaps_colours() {
    let spans = parse("\u{03}4,2a\u{16}b");
    assert_eq!(spans[1].fg, Some(2));
    assert_eq!(spans[1].bg, Some(4));
}

#[test]
fn strip_removes_codes() {
    assert_eq!(
        strip("\u{02}bold\u{02} \u{03}4,5red\u{0f} ok"),
        "bold red ok"
    );
    assert_eq!(strip("100%\u{03}"), "100%");
}
