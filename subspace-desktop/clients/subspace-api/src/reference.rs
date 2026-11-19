//! Scalar issue resolution. Recognition belongs to the contracts service.
pub trait IssueResolver {
    type Issue;
    type Error;
    fn resolve(&self, reference: &str) -> Result<Self::Issue, Self::Error>;
}
