//! Ordering of confirmed writes and background reads.
use std::collections::HashMap;
use subspace_framework::client_github::model::{GhItem, PrState};

/// Keep a confirmed write when an older response arrives. A merge is final
/// even if a subsequent read still reflects GitHub's pre-merge state.
pub(crate) fn preserve_written_state(
    items: &mut [GhItem],
    writes: &HashMap<String, (PrState, std::time::Instant)>,
    started: std::time::Instant,
) {
    for item in items {
        if let Some(&(state, written_at)) = writes.get(&item.key)
            && (state == PrState::Merged || written_at >= started)
        {
            item.state = state;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::preserve_written_state;
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
        let closed_at = HashMap::from([(key, (PrState::Closed, closed))]);
        preserve_written_state(&mut items, &closed_at, started);
        assert_eq!(items[0].state, PrState::Closed);
        assert_eq!(items[1].state, untouched);
        items[0].state = PrState::Open;
        preserve_written_state(&mut items, &HashMap::new(), started);
        assert_eq!(
            items[0].state,
            PrState::Open,
            "another account has no local close"
        );
        preserve_written_state(&mut items, &closed_at, closed + Duration::from_secs(1));
        assert_eq!(
            items[0].state,
            PrState::Open,
            "later reads can observe a reopen"
        );
        preserve_written_state(&mut items, &closed_at, closed);
        assert_eq!(
            items[0].state,
            PrState::Closed,
            "a tied timestamp favours the write"
        );
    }
}
