//! Provisional comments keep their own identity until a real activity read.
use subspace_api_client::model::{Comment, IssueActivity};

#[derive(Clone, Copy)]
pub(crate) enum Entry<'a> {
    Activity(&'a IssueActivity),
    Posted(&'a Comment),
}

impl<'a> Entry<'a> {
    pub(crate) fn comment(self) -> Option<&'a Comment> {
        match self {
            Self::Activity(e) => e.comment.as_ref(),
            Self::Posted(c) => Some(c),
        }
    }
    pub(crate) fn at(self) -> &'a str {
        match self {
            Self::Activity(e) => &e.display_at,
            Self::Posted(c) => &c.display_created_at,
        }
    }
    pub(crate) fn id(self) -> &'a str {
        match self {
            Self::Activity(e) => &e.id,
            Self::Posted(c) => &c.id,
        }
    }
    pub(crate) fn parent(self) -> Option<&'a str> {
        self.comment()?.parent_comment_id.as_deref()
    }
}
