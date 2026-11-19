//! Flat, memory-only activity queries. The caller retains the window between activations.
use chrono::{DateTime, FixedOffset, SecondsFormat, Utc};
mod reads;

pub(crate) const TIMELINE_PAGE_SIZE: u32 = 50;

const RECENT_DAYS: i64 = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityQuery {
    project: Option<String>,
    since: DateTime<FixedOffset>,
}

impl Default for ActivityQuery {
    fn default() -> Self {
        Self::at(Utc::now())
    }
}

impl ActivityQuery {
    /// Default seven-day window, with an injectable clock for deterministic callers.
    pub fn at(now: DateTime<Utc>) -> Self {
        Self {
            project: None,
            since: (now - chrono::Duration::days(RECENT_DAYS)).fixed_offset(),
        }
    }
    pub fn project(mut self, project: Option<&str>) -> Self {
        self.project = project
            .map(str::trim)
            .filter(|project| !project.is_empty())
            .map(str::to_owned);
        self
    }
    pub fn since(mut self, since: DateTime<FixedOffset>) -> Self {
        self.since = since;
        self
    }
    pub fn selected_project(&self) -> Option<&str> {
        self.project.as_deref()
    }
    // qual:api — the subsequent activity board displays the retained window.
    pub fn activity_since(&self) -> DateTime<FixedOffset> {
        self.since
    }

    fn parameters(&self, trackers: bool, cursor: Option<&str>) -> Vec<(String, String)> {
        let mut parameters = vec![
            ("order".into(), "activity".into()),
            ("limit".into(), "50".into()),
        ];
        parameters.extend(
            self.project
                .iter()
                .map(|project| ("project".into(), project.clone())),
        );
        parameters.extend(cursor.map(|cursor| ("cursor".into(), cursor.into())));
        if trackers {
            parameters.push(("type".into(), "tracker".into()));
            parameters.push(("stateType".into(), "started".into()));
        } else {
            parameters.push((
                "activitySince".into(),
                self.since.to_rfc3339_opts(SecondsFormat::Secs, false),
            ));
        }
        parameters
    }
}
