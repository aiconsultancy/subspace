//! Ages and dates for rows, tooltips and the Updated property.

const DAY: i64 = 86_400;
const HOUR: i64 = 3_600;
const MINUTE: i64 = 60;
const WEEK: i64 = 7 * DAY;
const MONTH: i64 = 30 * DAY;
const YEAR: i64 = 365 * DAY;
/// Weeks run to 8w before months start, so a month's step and the age at
/// which months begin differ.
const MONTH_FROM: i64 = 56 * DAY;

/// Each age unit with its step size and the age at which it starts.
const AGE_STEPS: [(&str, i64, i64); 6] = [
    ("y", YEAR, YEAR),
    ("mo", MONTH, MONTH_FROM),
    ("w", WEEK, WEEK),
    ("d", DAY, DAY),
    ("h", HOUR, HOUR),
    ("m", MINUTE, MINUTE),
];

/// Compact age for rows: "now", "5m", "3h", "2d", "5w", "4mo", "2y". Weeks run
/// to 8w before months start. `now` is Unix seconds.
pub fn age(iso: &str, now: i64) -> String {
    let Ok(time) = chrono::DateTime::parse_from_rfc3339(iso) else {
        return String::new();
    };
    let seconds = now - time.timestamp();
    for (unit, size, from) in AGE_STEPS {
        if seconds >= from {
            return format!("{}{unit}", seconds / size);
        }
    }
    "now".into()
}

/// "2026-09-30 14:05 UTC" for tooltips and the Updated property.
pub fn full_date(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|time| time.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|_| iso.to_string())
}

pub fn now_seconds() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::{DAY, HOUR, MINUTE, age, full_date};

    #[test]
    fn ages_step_from_minutes_to_years() {
        let iso = "2026-01-01T00:00:00Z";
        let base = chrono::DateTime::parse_from_rfc3339(iso)
            .expect("the fixture is valid RFC 3339")
            .timestamp();
        assert_eq!(age(iso, base + MINUTE / 2), "now");
        assert_eq!(age(iso, base + 5 * MINUTE), "5m");
        assert_eq!(age(iso, base + 3 * HOUR), "3h");
        assert_eq!(age(iso, base + 2 * DAY), "2d");
        assert_eq!(age(iso, base + 50 * DAY), "7w");
        assert_eq!(age(iso, base + 60 * DAY), "2mo");
        assert_eq!(age(iso, base + 800 * DAY), "2y");
        assert_eq!(age("garbage", base), "");
    }

    #[test]
    fn full_dates_read_as_utc_minutes_and_keep_unparsable_input() {
        assert_eq!(
            full_date("2026-09-30T14:05:09Z"),
            "2026-09-30 14:05 UTC".to_string()
        );
        assert_eq!(full_date("garbage"), "garbage".to_string());
    }
}
