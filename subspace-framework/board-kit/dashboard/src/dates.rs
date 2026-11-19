//! Service times as the lists and panels show them: the compact age a row
//! carries, the relative phrase a panel reads, and a full local stamp.
//!
//! Every dashboard shows times the same way, so the rules live here rather
//! than in each dashboard crate.

use chrono::{DateTime, Local, Utc};

pub fn parse_date(iso: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

/// Compact age for rows: "now", "5m", "3h", "2d", "5w", "4mo", "2y". An
/// unreadable time is empty rather than guessed at.
// qual:api
pub fn age(iso: &str, now: DateTime<Utc>) -> String {
    const DAY: i64 = 86_400;
    const STEPS: [(&str, i64, i64); 6] = [
        ("y", 365 * DAY, 365 * DAY),
        ("mo", 30 * DAY, 56 * DAY),
        ("w", 7 * DAY, 7 * DAY),
        ("d", DAY, DAY),
        ("h", 3600, 3600),
        ("m", 60, 60),
    ];
    let Some(date) = parse_date(iso) else {
        return String::new();
    };
    let seconds = (now - date).num_seconds();
    STEPS
        .iter()
        .find(|(_, _, from)| seconds >= *from)
        .map_or_else(
            || "now".into(),
            |(unit, size, _)| format!("{}{unit}", seconds / size),
        )
}

/// "3 days ago", "yesterday", "just now".
pub fn relative_time(iso: &str, now: DateTime<Utc>) -> String {
    const STEPS: [(&str, i64); 6] = [
        ("year", 31_536_000),
        ("month", 2_592_000),
        ("week", 604_800),
        ("day", 86_400),
        ("hour", 3600),
        ("minute", 60),
    ];
    let Some(date) = parse_date(iso) else {
        return String::new();
    };
    let seconds = (date - now).num_seconds();
    for (unit, size) in STEPS {
        if seconds.abs() >= size {
            let n = (seconds as f64 / size as f64).round() as i64;
            return match (unit, n) {
                ("day", -1) => "yesterday".into(),
                ("day", 1) => "tomorrow".into(),
                (_, -1) => format!("last {unit}"),
                (_, 1) => format!("next {unit}"),
                (_, n) if n < 0 => format!("{} {unit}s ago", -n),
                (_, n) => format!("in {n} {unit}s"),
            };
        }
    }
    "just now".into()
}

/// A full local time for a panel, such as "7 Oct 2026 09:30".
pub fn stamp(iso: &str) -> String {
    parse_date(iso).map_or_else(String::new, |date| {
        date.with_timezone(&Local)
            .format("%-d %b %Y %H:%M")
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::{age, parse_date, relative_time, stamp};

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-07T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn an_age_shortens_to_the_largest_whole_unit() {
        assert_eq!(age("2026-10-07T11:59:30Z", now()), "now");
        assert_eq!(age("2026-10-07T11:55:00Z", now()), "5m");
        assert_eq!(age("2026-10-07T09:00:00Z", now()), "3h");
        assert_eq!(age("2026-10-05T12:00:00Z", now()), "2d");
        assert_eq!(age("2026-09-23T12:00:00Z", now()), "2w");
        assert_eq!(age("whenever", now()), "");
    }

    #[test]
    fn a_relative_phrase_names_the_nearest_unit() {
        assert_eq!(relative_time("2026-10-07T11:59:30Z", now()), "just now");
        assert_eq!(relative_time("2026-10-06T12:00:00Z", now()), "yesterday");
        assert_eq!(relative_time("2026-10-04T12:00:00Z", now()), "3 days ago");
        assert_eq!(relative_time("2026-10-08T12:00:00Z", now()), "tomorrow");
        assert_eq!(relative_time("whenever", now()), "");
    }

    #[test]
    fn a_stamp_is_empty_rather_than_guessed() {
        assert!(stamp("2026-10-07T09:30:00Z").contains("2026"));
        assert_eq!(stamp(""), "");
        assert_eq!(stamp("whenever"), "");
        assert!(parse_date("2026-10-07T09:30:00Z").is_some());
        assert!(parse_date("whenever").is_none());
    }
}
