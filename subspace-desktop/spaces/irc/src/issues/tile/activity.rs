//! Pure UTC formatting. The hub samples the clock outside rendering.
use chrono::{DateTime, Duration, NaiveDate, Utc};
const RECENT: Duration = Duration::hours(1);
const DATE_AFTER: i64 = 7;

pub(crate) struct Relative {
    pub label: String,
    pub recent: bool,
}

pub(crate) fn relative(occurred_at: &str, now: DateTime<Utc>) -> Option<Relative> {
    let at = DateTime::parse_from_rfc3339(occurred_at)
        .ok()?
        .with_timezone(&Utc);
    let elapsed = now.signed_duration_since(at);
    let label = if elapsed < Duration::zero() {
        "just now".into()
    } else if elapsed < Duration::hours(1) {
        format!("{}m ago", elapsed.num_minutes())
    } else if elapsed < Duration::days(1) {
        format!("{}h ago", elapsed.num_hours())
    } else if elapsed < Duration::days(DATE_AFTER) {
        format!("{}d ago", elapsed.num_days())
    } else {
        at.format("%d %b %Y").to_string()
    };
    Some(Relative {
        label,
        recent: elapsed >= Duration::zero() && elapsed < RECENT,
    })
}
pub(super) fn due(value: &str, today: NaiveDate) -> Option<String> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .or_else(|| {
            DateTime::parse_from_rfc3339(value)
                .ok()
                .map(|at| at.date_naive())
        })?;
    let days = date.signed_duration_since(today).num_days();
    let formatted = if (0..DATE_AFTER).contains(&days) {
        date.format("%a").to_string()
    } else {
        date.format("%d %b %Y").to_string()
    };
    Some(format!("Due {formatted}"))
}
