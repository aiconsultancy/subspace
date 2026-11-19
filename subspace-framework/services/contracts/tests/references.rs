use subspace_contracts::issues::recognition::ReferenceRecognizer;

#[test]
fn references_preserve_utf8_byte_ranges_and_input_order() {
    let recognizer = ReferenceRecognizer::new(["TST", "OPS"], ["FALLBACK"]);
    let text = "é (TST-1612), OPS-42 and TST-1612";
    let found = recognizer.find(text);
    assert_eq!(found.len(), 3);
    assert_eq!(found[0].range(), 4..12);
    assert_eq!(
        found.iter().map(|r| r.identifier()).collect::<Vec<_>>(),
        ["TST-1612", "OPS-42", "TST-1612"]
    );
    for reference in found {
        assert_eq!(&text[reference.range()], reference.identifier());
    }
}

#[test]
fn only_known_keys_and_complete_identifiers_match() {
    let recognizer = ReferenceRecognizer::new(["OPS"], ["TST"]);
    assert!(
        recognizer
            .find("TST-1 XOPS-1 OPS-1x OPS-1_2 OPS-0 OPS-01 éOPS-2 OPS-3é")
            .is_empty()
    );
    assert_eq!(recognizer.find("[OPS-4] / OPS-12").len(), 2);
    assert_eq!(ReferenceRecognizer::new([], ["TST"]).find("TST-2").len(), 1);
}

#[test]
fn urls_and_code_spans_are_not_issue_links() {
    let recognizer = ReferenceRecognizer::new([], ["TST"]);
    let text = "https://example.test/TST-1 `TST-2` ``TST-3`` <https://x.test?q=TST-4> TST-5 www.example.test/TST-6 irc://host/TST-7 mailto:TST-8@example.test";
    let found = recognizer.find(text);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].identifier(), "TST-5");
    assert!(recognizer.find("```\nTST-2\n``` ").is_empty());
}

#[test]
fn hyphenated_words_and_dates_are_not_issue_references() {
    let recognizer = ReferenceRecognizer::new([], ["TST"]);
    assert!(
        recognizer
            .find("PRE-TST-12 re-TST-5 TST-2026-11-15 -TST-6 éTST-7")
            .is_empty()
    );
    assert_eq!(recognizer.find("(TST-12), TST-5.").len(), 2);
}

#[test]
fn unmatched_backtick_runs_are_literal() {
    let recognizer = ReferenceRecognizer::new([], ["TST"]);
    for text in ["` TST-12", "`` TST-12 ` TST-5", "`TST-1` ` TST-12"] {
        let found = recognizer.find(text);
        assert_eq!(found[0].identifier(), "TST-12");
    }
    assert_eq!(recognizer.find("`` TST-12 ` TST-5").len(), 2);
}

#[test]
fn closed_code_spans_after_unmatched_runs_are_still_protected() {
    let recognizer = ReferenceRecognizer::new([], ["TST"]);
    assert!(recognizer.find("` ``TST-1``").is_empty());
    let found = recognizer.find("``` `TST-2` TST-3 ``TST-4`` TST-5");
    assert_eq!(
        found.iter().map(|r| r.identifier()).collect::<Vec<_>>(),
        ["TST-3", "TST-5"]
    );
}
#[test]
fn standard_encoding_and_date_names_stay_plain_with_matching_project_keys() {
    let recognizer = ReferenceRecognizer::new(["UTF", "ISO"], ["TST"]);
    assert!(recognizer.find("UTF-8 ISO-8601").is_empty());
    assert_eq!(recognizer.find("UTF-9 ISO-42").len(), 2);
}

#[test]
fn invalid_loaded_and_fallback_keys_are_discarded_before_recognition() {
    let invalid = [
        "",
        " ",
        "tst",
        "1TST",
        "T-ST",
        "T_ST",
        "éTST",
        "TST!",
        "A",
        "TOOLONGKEY1",
    ];
    let recognizer = ReferenceRecognizer::new(invalid, ["TST"]);
    assert_eq!(recognizer.find("TST-5").len(), 1);
    assert!(recognizer.find("-5 tst-5 1TST-5 éTST-5 T_ST-5").is_empty());
    let recognizer = ReferenceRecognizer::new([], invalid);
    assert!(
        recognizer
            .find("-5 tst-5 1TST-5 éTST-5 T_ST-5 A-5 TOOLONGKEY1-5 TST-5")
            .is_empty()
    );
}

#[test]
fn recognises_uses_the_selected_keys_and_complete_identifier_grammar() {
    let references = ReferenceRecognizer::new(["bad"], ["TST"]);
    assert!(references.recognises("TST-827"));
    for identifier in [
        "OPS-1",
        "TST-0",
        "TST-0827",
        "TST-827x",
        "TST-827/slug",
        "TST-827 TST-828",
    ] {
        assert!(!references.recognises(identifier), "{identifier}");
    }
    let references = ReferenceRecognizer::new(["OPS"], ["TST"]);
    assert!(references.recognises("OPS-1"));
    assert!(!references.recognises("TST-827"));
    assert!(!ReferenceRecognizer::new([], []).recognises("TST-827"));
}
