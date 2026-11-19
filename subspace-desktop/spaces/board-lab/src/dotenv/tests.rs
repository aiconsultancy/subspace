use super::{MAX_INPUT_BYTES, MAX_VALUE_BYTES, MAX_VARIABLES, parse, parse_reader, valid_name};

#[test]
fn literal_values_quotes_comments_and_export_prefix_are_preserved() {
    let input = "# local settings\r\nexport API_TOKEN = 'test-only=a#b'\r\nURL=https://example.invalid/a#b # comment\nEMPTY=\nTEXT=\"line\\nnext\\tpart\\\"\"\n";
    let values = parse(input).expect("literal dotenv").into_values();
    assert_eq!(values["API_TOKEN"], "test-only=a#b");
    assert_eq!(values["URL"], "https://example.invalid/a#b");
    assert_eq!(values["EMPTY"], "");
    assert_eq!(values["TEXT"], "line\nnext\tpart\"");
}

#[test]
fn duplicate_names_expansions_and_malformed_lines_are_rejected_without_values() {
    for source in [
        "TOKEN=test-only-first\nTOKEN=test-only-second",
        "TOKEN=test-only-first\ntoken=test-only-second",
        "TOKEN=$(not-a-command)",
        "TOKEN=${SAVED_TOKEN}",
        "TOKEN=`not-a-command`",
        "TOKEN='test-only-unclosed",
        "TOKEN=\"test-only\" trailing-text",
        "TOKEN=bad\0value",
        "TOKEN_WITHOUT_EQUALS",
        "1TOKEN=test-only-invalid-name",
    ] {
        let error = parse(source).expect_err("invalid dotenv");
        let diagnostic = format!("{error} {error:?}");
        assert!(
            !diagnostic.contains("test-only"),
            "diagnostic leaked a value"
        );
        assert!(diagnostic.contains("Dotenv"));
    }
}

#[test]
fn imported_values_debug_lists_names_not_their_values() {
    let values = parse("API_TOKEN=test-only-secret-value").expect("literal dotenv");
    let debug = format!("{values:?}");
    assert!(debug.contains("API_TOKEN"));
    assert!(!debug.contains("test-only-secret-value"));
}

#[test]
fn input_value_and_count_budgets_are_enforced() {
    assert!(parse(&"#".repeat(MAX_INPUT_BYTES + 1)).is_err());
    assert!(parse(&format!("TOKEN={}", "a".repeat(MAX_VALUE_BYTES))).is_ok());
    assert!(parse(&format!("TOKEN={}", "a".repeat(MAX_VALUE_BYTES + 1))).is_err());
    let maximum = (0..MAX_VARIABLES)
        .map(|i| format!("KEY_{i}=value\n"))
        .collect::<String>();
    assert_eq!(
        parse(&maximum).expect("maximum entries").names().count(),
        MAX_VARIABLES
    );
    assert!(parse(&(maximum + "EXTRA=value")).is_err());
}

#[test]
fn portable_names_are_ascii_and_nonempty() {
    assert!(valid_name("_API_TOKEN_2"));
    for invalid in ["", "2TOKEN", "API-TOKEN", "API TOKEN", "TÖKEN"] {
        assert!(!valid_name(invalid));
    }
}

#[test]
fn reader_stops_at_the_input_budget_and_does_not_echo_invalid_bytes() {
    let input = vec![b'#'; MAX_INPUT_BYTES * 2];
    let mut reader = std::io::Cursor::new(input);
    assert!(parse_reader(&mut reader).is_err());
    assert_eq!(reader.position(), MAX_INPUT_BYTES as u64 + 1);
    assert!(parse_reader(std::io::Cursor::new([0xff, 0xfe])).is_err());
    let values =
        parse_reader(std::io::Cursor::new(b"TOKEN=test-only-value")).expect("literal import");
    assert_eq!(values.into_values()["TOKEN"], "test-only-value");
}
