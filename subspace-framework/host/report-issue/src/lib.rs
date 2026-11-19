//! Explicit-consent issue reporting into the GitHub repository the app
//! names, for apps built with the `report-issue` feature.

mod attachment;
mod attachments;
mod diagnostics_view;
mod form;
mod image_actions;
mod images_view;
mod report;
mod submission;
mod view;

#[cfg(feature = "test-support")]
mod fixture;
#[cfg(test)]
mod tests;

pub use attachment::Attachment;
pub use report::{Destination, ReportIssue, ReportIssueEvent};
pub use subspace_client_github::issue_report::{
    Diagnostics, Draft, IssuePaths, IssueRepository, prepare,
};
