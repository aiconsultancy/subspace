//! Times the Copilot Agents list and panel show. The compact age and the local
//! stamp are shared rules ([`subspace_framework::dashboard::dates`]).

pub use subspace_framework::dashboard::dates::{age, stamp};

use subspace_framework::dashboard::dates::parse_date;

/// A session-log entry's `created` time, which the Copilot log gives as
/// whole seconds since the epoch, as the stamp every panel shows.
pub fn unix_stamp(created: i64) -> String {
    chrono::DateTime::from_timestamp(created, 0)
        .map_or_else(String::new, |date| stamp(&date.to_rfc3339()))
}

/// The later of two ISO times, as text. An unreadable time loses to a
/// readable one, so a row still sorts by something real.
pub fn latest(first: &str, second: &str) -> String {
    match (parse_date(first), parse_date(second)) {
        (Some(a), Some(b)) if b > a => second.to_owned(),
        (Some(_), _) => first.to_owned(),
        (None, Some(_)) => second.to_owned(),
        (None, None) => first.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::latest;

    #[test]
    fn a_unix_time_reads_as_the_stamp_the_panels_show() {
        let stamp = super::unix_stamp(1_790_000_000);
        assert!(stamp.contains("2026"), "{stamp}");
        assert_eq!(super::unix_stamp(i64::MAX), "");
    }

    #[test]
    fn the_later_of_two_times_wins_and_an_unreadable_one_loses() {
        let early = "2026-10-01T00:00:00Z";
        let late = "2026-10-05T00:00:00Z";
        assert_eq!(latest(early, late), late);
        assert_eq!(latest(late, early), late);
        assert_eq!(latest("whenever", late), late);
        assert_eq!(latest(late, "whenever"), late);
        assert_eq!(latest("whenever", "nonsense"), "whenever");
    }
}
