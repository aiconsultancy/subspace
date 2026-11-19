//! A media list read again after a change: pages update what they name in
//! place and add what is new, and only a scan that reaches its last page
//! without a failure retires the items it never saw and takes its order. A
//! failed or unfinished scan keeps everything shown.
use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub(super) struct Rescan {
    /// Ids seen since the scan began, in the API's order; `None` while no
    /// scan of a held list is under way.
    seen: Option<Vec<String>>,
}

impl Rescan {
    /// A scan of the held list starts from its first page.
    pub(super) fn begin(&mut self) {
        self.seen = Some(Vec::new());
    }

    /// The scan failed: nothing shown is retired until a new one begins.
    pub(super) fn abandon(&mut self) {
        self.seen = None;
    }

    /// Takes one page: changed items are replaced, new ones appended. On the
    /// last page of a scan, unseen items are retired and the API's order kept.
    pub(super) fn absorb<T>(
        &mut self,
        items: &mut Vec<T>,
        page: Vec<T>,
        last: bool,
        id: impl Fn(&T) -> &str,
    ) {
        for item in page {
            if let Some(seen) = self.seen.as_mut() {
                seen.push(id(&item).to_owned());
            }
            match items.iter_mut().find(|held| id(held) == id(&item)) {
                Some(held) => *held = item,
                None => items.push(item),
            }
        }
        if last && let Some(seen) = self.seen.take() {
            let kept: HashSet<&str> = seen.iter().map(String::as_str).collect();
            items.retain(|item| kept.contains(id(item)));
            items.sort_by_key(|item| seen.iter().position(|seen| seen == id(item)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Rescan;

    fn absorb(scan: &mut Rescan, items: &mut Vec<(String, u32)>, page: &[(&str, u32)], last: bool) {
        let page = page.iter().map(|(id, v)| ((*id).to_owned(), *v)).collect();
        scan.absorb(items, page, last, |item| item.0.as_str());
    }

    fn held() -> Vec<(String, u32)> {
        vec![("a".into(), 1), ("b".into(), 1), ("c".into(), 1)]
    }

    #[test]
    fn a_complete_scan_retires_missing_items_and_updates_changed_ones() {
        let mut items = held();
        let mut scan = Rescan::default();
        scan.begin();
        absorb(&mut scan, &mut items, &[("c", 2), ("d", 1)], false);
        // Mid-scan nothing is retired yet.
        assert_eq!(items.len(), 4);
        absorb(&mut scan, &mut items, &[("a", 1)], true);
        assert_eq!(
            items,
            vec![("c".into(), 2), ("d".into(), 1), ("a".into(), 1)]
        );
    }

    #[test]
    fn a_failed_scan_keeps_the_old_items() {
        let mut items = held();
        let mut scan = Rescan::default();
        scan.begin();
        absorb(&mut scan, &mut items, &[("a", 2)], false);
        scan.abandon();
        absorb(&mut scan, &mut items, &[], true);
        assert_eq!(
            items,
            vec![("a".into(), 2), ("b".into(), 1), ("c".into(), 1)]
        );
    }
}
