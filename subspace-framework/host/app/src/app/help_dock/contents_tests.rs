use super::{Heading, HeadingLevel, HelpSections, plain_title};

fn heading(level: HeadingLevel, title: &str, section: usize) -> Heading {
    Heading {
        level,
        title: title.to_owned(),
        section,
    }
}

#[test]
fn only_h1_and_h2_headings_start_sections_in_document_order() {
    let document = HelpSections::parse(
        "Intro text\n\n# Guide\nBody\n## Setup\n### Details\nMore\n#### Deep\n## Keys\nLast\n",
    );
    assert_eq!(
        document.headings(),
        [
            heading(HeadingLevel::One, "Guide", 1),
            heading(HeadingLevel::Two, "Setup", 2),
            heading(HeadingLevel::Two, "Keys", 3),
        ]
    );
    assert_eq!(
        document.sections(),
        [
            "Intro text\n\n",
            "# Guide\nBody\n",
            "## Setup\n### Details\nMore\n#### Deep\n",
            "## Keys\nLast\n",
        ]
    );
    assert!(document.shows_contents());
    assert_eq!(
        document.heading_for(2).map(|h| h.title.as_str()),
        Some("Setup")
    );
    assert!(document.heading_for(0).is_none());
}

#[test]
fn a_document_that_starts_with_a_heading_has_no_preamble_section() {
    let document = HelpSections::parse("\n# One\n## Two\n");
    assert_eq!(document.sections(), ["# One\n", "## Two\n"]);
    assert_eq!(document.headings()[0].section, 0);
    assert!(!document.shows_contents(), "two headings are too few");
}

#[test]
fn duplicate_titles_keep_distinct_targets() {
    let document = HelpSections::parse("# Board\n## Setup\nA\n# Board\n## Setup\nB\n");
    let targets: Vec<_> = document
        .headings()
        .iter()
        .map(|h| (h.title.as_str(), h.section))
        .collect();
    assert_eq!(
        targets,
        [("Board", 0), ("Setup", 1), ("Board", 2), ("Setup", 3)]
    );
    assert_eq!(document.sections()[3], "## Setup\nB\n");
}

#[test]
fn headings_inside_code_fences_and_non_headings_are_text() {
    let document = HelpSections::parse(
        "# Real\n```bash\n# comment\n## not a heading\n```\n~~~~\n# tilde\n```\n# still code\n~~~~\n#hashtag\n    # indented code\n####### seven\n## After\n",
    );
    let titles: Vec<_> = document
        .headings()
        .iter()
        .map(|h| h.title.as_str())
        .collect();
    assert_eq!(titles, ["Real", "After"]);
    assert_eq!(document.sections().len(), 2);
}

#[test]
fn heading_titles_lose_inline_formatting() {
    assert_eq!(plain_title(" **Bold** and *em* "), "Bold and em");
    assert_eq!(
        plain_title("Use `subspace --help` now"),
        "Use subspace --help now"
    );
    assert_eq!(
        plain_title("See [the docs](https://example.invalid/a_b) here"),
        "See the docs here"
    );
    assert_eq!(plain_title("![logo](x.png) Title"), "logo Title");
    assert_eq!(
        plain_title("_under_ and snake_case"),
        "under and snake_case"
    );
    assert_eq!(plain_title("~~old~~ <em>new</em>"), "old new");
    assert_eq!(plain_title(r"Escaped \*star\* \#"), "Escaped *star* #");
    assert_eq!(plain_title("Closing hashes ##"), "Closing hashes");
    assert_eq!(plain_title("C# notes"), "C# notes");
    assert_eq!(plain_title("a < b"), "a < b");
    assert_eq!(plain_title(" ## "), "");
}

#[test]
fn parsed_headings_use_plain_titles() {
    let document = HelpSections::parse("# The `github` *board*\n## [Setup](#setup)\n## Keys #\n");
    let titles: Vec<_> = document
        .headings()
        .iter()
        .map(|h| h.title.as_str())
        .collect();
    assert_eq!(titles, ["The github board", "Setup", "Keys"]);
}

#[test]
fn the_host_image_fixture_has_useful_headings_and_round_trips() {
    let guide = super::super::fixtures::GUIDE;
    crate::test_support::assert_help_contents(guide);
    assert_eq!(HelpSections::parse(guide).headings().len(), 3);
}
