//! The one-shot crew detail read.

use serde::Deserialize;

use crate::Error;
use crate::model::CrewMember;

/// The member and their projects, identified by their ship's registration.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct CrewDetail {
    pub ship: String,
    pub member: CrewMember,
    #[serde(default, rename = "taskText")]
    task_text: serde_json::Value,
}

impl CrewDetail {
    /// Whether this reply positively confirms it carries full task text.
    /// Missing and unrecognised markers don't invalidate a usable reply.
    pub fn has_full_task_text(&self) -> bool {
        self.task_text.as_str() == Some("full")
    }

    pub(crate) fn matching(self, ship: &str, member: &str) -> Result<Self, Error> {
        if self.ship != ship || self.member.id != member {
            return Err(Error::InvalidResponse);
        }
        Ok(self)
    }
}
