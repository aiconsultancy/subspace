use super::ReadError;
use std::collections::{BTreeMap, VecDeque};

const CAPACITY: usize = 500;

/// Retryable diagnostics, independent of the resolved-card cache. Oldest
/// completed failures leave first; restarting a key removes only that key.
#[derive(Default)]
pub(in crate::hub) struct ReferenceErrors {
    entries: BTreeMap<String, ReadError>,
    order: VecDeque<String>,
}

impl ReferenceErrors {
    pub(in crate::hub) fn entries(&self) -> &BTreeMap<String, ReadError> {
        &self.entries
    }
    pub(in crate::hub) fn insert(&mut self, identifier: &str, error: ReadError) {
        self.remove(identifier);
        if self.entries.len() == CAPACITY
            && let Some(oldest) = self.order.pop_front()
        {
            self.entries.remove(&oldest);
        }
        self.order.push_back(identifier.into());
        self.entries.insert(identifier.into(), error);
    }
    pub(in crate::hub) fn remove(&mut self, identifier: &str) {
        self.entries.remove(identifier);
        self.order.retain(|id| id != identifier);
    }
    pub(in crate::hub) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::ReferenceErrors;
    use crate::hub::ReadError;

    #[test]
    fn errors_are_bounded_and_updating_a_key_keeps_it_recent() {
        let mut errors = ReferenceErrors::default();
        for number in 1..=500 {
            errors.insert(&format!("SUB-{number}"), ReadError::Network);
        }
        errors.insert("SUB-1", ReadError::SignInNeeded);
        errors.insert("SUB-501", ReadError::Network);
        assert_eq!(errors.entries().len(), 500);
        assert_eq!(
            errors.entries().get("SUB-1"),
            Some(&ReadError::SignInNeeded)
        );
        assert!(!errors.entries().contains_key("SUB-2"));
        assert!(errors.entries().contains_key("SUB-501"));
    }

    #[test]
    fn retry_removes_only_its_identifier_and_scope_clear_removes_everything() {
        let mut errors = ReferenceErrors::default();
        errors.insert("SUB-1", ReadError::Network);
        errors.insert("SUB-2", ReadError::SignInNeeded);
        errors.remove("SUB-1");
        assert!(!errors.entries().contains_key("SUB-1"));
        assert_eq!(
            errors.entries().get("SUB-2"),
            Some(&ReadError::SignInNeeded)
        );
        errors.clear();
        assert!(errors.entries().is_empty());
    }
}
