use super::Counters;
use std::path::Path;
pub(super) const SUPPORTED: bool = false;
pub(super) struct Mapping(Counters);
impl Mapping {
    pub fn open(_: &Path) -> Result<Self, String> {
        Ok(Self(Counters::default()))
    }
    pub fn counters(&self) -> &Counters {
        &self.0
    }
}
