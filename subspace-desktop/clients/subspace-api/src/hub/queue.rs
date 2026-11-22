use std::collections::VecDeque;

/// FIFO inside each tier; queued visible work always precedes references and pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadPriority {
    Visible,
    Reference,
    LaterPage,
}

pub(super) struct PriorityQueue<T> {
    tiers: [VecDeque<T>; 3],
}
impl<T> Default for PriorityQueue<T> {
    fn default() -> Self {
        Self {
            tiers: std::array::from_fn(|_| VecDeque::new()),
        }
    }
}
impl<T> PriorityQueue<T> {
    pub(super) fn push(&mut self, priority: ReadPriority, value: T) {
        self.tiers[priority as usize].push_back(value);
    }
    #[cfg(test)]
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.tiers.iter().all(VecDeque::is_empty)
    }
    pub(super) fn pop(&mut self) -> Option<T> {
        self.pop_for(true)
    }
    pub(super) fn pop_for(&mut self, references: bool) -> Option<T> {
        self.tiers
            .iter_mut()
            .enumerate()
            .filter(|(tier, _)| references || *tier != ReadPriority::Reference as usize)
            .find_map(|(_, tier)| tier.pop_front())
    }
}

#[cfg(test)]
mod tests {
    use super::{PriorityQueue, ReadPriority};
    #[test]
    fn visible_then_references_then_pages_with_fifo_ties() {
        let mut queue = PriorityQueue::default();
        queue.push(ReadPriority::LaterPage, 4);
        queue.push(ReadPriority::Reference, 3);
        queue.push(ReadPriority::Visible, 1);
        queue.push(ReadPriority::Visible, 2);
        assert_eq!(
            (queue.pop(), queue.pop(), queue.pop(), queue.pop()),
            (Some(1), Some(2), Some(3), Some(4))
        );
        assert_eq!(queue.pop(), None);
    }
}
