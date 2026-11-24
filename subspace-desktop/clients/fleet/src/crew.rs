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
}

impl CrewDetail {
    pub(crate) fn matching(self, ship: &str, member: &str) -> Result<Self, Error> {
        if self.ship != ship || self.member.id != member {
            return Err(Error::InvalidResponse);
        }
        Ok(self)
    }
}
