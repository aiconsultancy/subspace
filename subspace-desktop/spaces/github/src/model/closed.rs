//! Ordering of confirmed closes and background reads.
use std::collections::HashMap;
use subspace_framework::client_github::model::{GhItem, PrState};

/// Keep a confirmed close when an older response arrives.
pub(crate) fn preserve_closed(
    items: &mut [GhItem],
    closed_at: &HashMap<String, std::time::Instant>,
    started: std::time::Instant,
) {
    for item in items {
        if closed_at
            .get(&item.key)
            .is_some_and(|closed| *closed >= started)
        {
            item.state = PrState::Closed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::preserve_closed;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};
    use subspace_framework::client_github::model::PrState;

    #[test]
    fn only_the_same_key_and_account_with_an_older_read_keep_the_close() {
        let started = Instant::now();
        let closed = started + Duration::from_secs(1);
        let mut items = crate::sample::items();
        let key = items[0].key.clone();
        let untouched = items[1].state;
        let closed_at = HashMap::from([(key, closed)]);
        preserve_closed(&mut items, &closed_at, started);
        assert_eq!(items[0].state, PrState::Closed);
        assert_eq!(items[1].state, untouched);
        items[0].state = PrState::Open;
        preserve_closed(&mut items, &HashMap::new(), started);
        assert_eq!(
            items[0].state,
            PrState::Open,
            "another account has no local close"
        );
        preserve_closed(&mut items, &closed_at, closed + Duration::from_secs(1));
        assert_eq!(
            items[0].state,
            PrState::Open,
            "later reads can observe a reopen"
        );
        preserve_closed(&mut items, &closed_at, closed);
        assert_eq!(
            items[0].state,
            PrState::Closed,
            "a tied timestamp favours the write"
        );
    }
}
