//! Navigation to a decision by its project-scoped reference. The answering
//! board opens and reads it in its own scope.

use crate::{Contract, Request};

const LINK_PREFIX: &str = "subspace-decision:";
const NUMBER_DIGITS: std::ops::RangeInclusive<usize> = 5..=10;
const PROJECT_KEY_LENGTH: std::ops::RangeInclusive<usize> = 2..=10;

pub struct Decisions;

impl Contract for Decisions {
    const NAME: &'static str = "decisions";
    const VERSION: u32 = 1;
}

/// Navigation capability used by the host's board navigation broker.
// qual:api - Declared by app Spaces through the Framework facade.
pub struct OpenDecision;

impl Request for OpenDecision {
    type Contract = Decisions;
    const NAME: &'static str = "open";
    type Response = ();
}

/// A decision reference such as `D-00042-EXA`, ignoring ASCII case.
pub fn is_identifier(identifier: &str) -> bool {
    let mut parts = identifier.split('-');
    let (Some(prefix), Some(number), Some(key), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    prefix.eq_ignore_ascii_case("D")
        && NUMBER_DIGITS.contains(&number.len())
        && number.bytes().all(|byte| byte.is_ascii_digit())
        && PROJECT_KEY_LENGTH.contains(&key.len())
        && key.starts_with(|ch: char| ch.is_ascii_alphabetic())
        && key.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

pub fn link(identifier: &str) -> Option<String> {
    is_identifier(identifier).then(|| format!("{LINK_PREFIX}{identifier}"))
}

// qual:api - The answering Space validates navigation received through the broker.
pub fn from_link(link: &str) -> Option<&str> {
    link.strip_prefix(LINK_PREFIX)
        .filter(|identifier| is_identifier(identifier))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_round_trip_without_accepting_extra_segments() {
        for identifier in ["d-00001-ex", "D-100000-PNG"] {
            assert_eq!(from_link(&link(identifier).unwrap()), Some(identifier));
        }
        for identifier in ["D-1-EX", "D-00001-X", "D-00001-EX-more", "D-00001-EX?q=x"] {
            assert!(link(identifier).is_none());
        }
        assert!(from_link("https://example.com/D-00001-EX").is_none());
    }
}
