//! Identity checks for asynchronous results. This does not stop remote work;
//! the owner keeps its tasks and decides when to discard an old result.

/// One attempt for a typed target, captured when the request starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request<K> {
    generation: u64,
    key: K,
}

impl<K> Request<K> {
    pub fn key(&self) -> &K {
        &self.key
    }
}

/// The current attempt in one independent request lane.
// qual:api
#[derive(Debug)]
pub struct KeyedRequest<K> {
    generation: u64,
    current: Option<Request<K>>,
}

// qual:allow(dry, boilerplate) reason: "Derived Default would require K: Default, although an empty request lane has no key."
impl<K> Default for KeyedRequest<K> {
    fn default() -> Self {
        Self {
            generation: 0,
            current: None,
        }
    }
}

impl<K: Clone + PartialEq> KeyedRequest<K> {
    /// Replaces the previous attempt, even for the same target.
    pub fn begin(&mut self, key: K) -> Request<K> {
        self.generation = self.generation.wrapping_add(1);
        let request = Request {
            generation: self.generation,
            key,
        };
        self.current = Some(request.clone());
        request
    }

    // qual:api
    pub fn is_current(&self, request: &Request<K>) -> bool {
        self.current.as_ref() == Some(request)
    }

    /// Rejects completions from the current attempt without stopping its work.
    pub fn invalidate(&mut self) {
        self.current = None;
    }
}

#[cfg(test)]
mod tests {
    use super::KeyedRequest;

    #[test]
    fn only_the_latest_attempt_for_the_current_target_is_accepted() {
        let mut lane = KeyedRequest::default();
        let first = lane.begin("first");
        assert_eq!(first.key(), &"first");
        assert!(lane.is_current(&first));
        let second = lane.begin("first");
        assert!(!lane.is_current(&first));
        assert!(lane.is_current(&second));
        let third = lane.begin("other");
        assert!(!lane.is_current(&second));
        assert!(lane.is_current(&third));
    }

    #[test]
    fn invalidation_rejects_an_attempt_even_if_its_target_returns() {
        let mut lane = KeyedRequest::default();
        let old = lane.begin("target");
        lane.invalidate();
        assert!(!lane.is_current(&old));
        let new = lane.begin("target");
        assert!(!lane.is_current(&old));
        assert!(lane.is_current(&new));
    }
}
