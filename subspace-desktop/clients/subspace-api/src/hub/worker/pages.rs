//! Paged collections a live re-read reads again: fresh first pages are
//! followed with their own cursors as far as the person had paged, so the
//! re-read is installed whole and nothing shown is kept from an older read.
use crate::{Error, model::Page};

/// Appends the rows not already held, by id.
pub(in crate::hub) fn extend_unique<T>(
    items: &mut Vec<T>,
    incoming: Vec<T>,
    id: impl Fn(&T) -> &str,
) {
    for item in incoming {
        if !items.iter().any(|loaded| id(loaded) == id(&item)) {
            items.push(item);
        }
    }
}

/// How far a re-read pages: the rows shown before it, in pages of `size`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::hub) struct Extent {
    pub rows: usize,
    pub size: usize,
}

/// Reads continuation pages after a fresh first page until it holds
/// `extent.rows` rows, its cursor runs out, or it has read as many pages as
/// the held rows past the first page fill. Every page read here comes from
/// the fresh first page's cursor chain, never an older one. Any failure
/// fails the whole collection: the caller keeps what is shown.
pub(in crate::hub) fn fill<T>(
    items: &mut Vec<T>,
    cursor: &mut Option<String>,
    extent: Extent,
    mut next: impl FnMut(&str) -> Result<Page<T>, Error>,
    id: impl Fn(&T) -> &str,
) -> Result<(), Error> {
    let mut budget = extent
        .rows
        .saturating_sub(items.len())
        .div_ceil(extent.size.max(1));
    while items.len() < extent.rows && budget > 0 {
        let Some(at) = cursor.take() else {
            break;
        };
        let page = next(&at)?;
        budget -= 1;
        *cursor = page.next_cursor;
        for item in page.items {
            if !items.iter().any(|loaded| id(loaded) == id(&item)) {
                items.push(item);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Extent, fill};
    use crate::{Error, model::Page};

    fn page(ids: &[&str], cursor: Option<&str>) -> Page<String> {
        serde_json::from_value(serde_json::json!({"items":ids,"nextCursor":cursor})).unwrap()
    }

    /// Pages served by cursor: `c-1` → 3, 4; `c-2` → 5.
    fn served(cursor: &str) -> Result<Page<String>, Error> {
        match cursor {
            "c-1" => Ok(page(&["3", "4"], Some("c-2"))),
            "c-2" => Ok(page(&["5"], None)),
            _ => Err(Error::UnexpectedStatus(400)),
        }
    }

    fn filled(rows: usize) -> (Vec<String>, Option<String>, usize) {
        let mut items = vec!["1".to_owned(), "2".to_owned()];
        let mut cursor = Some("c-1".to_owned());
        let mut reads = 0;
        let extent = Extent { rows, size: 2 };
        fill(
            &mut items,
            &mut cursor,
            extent,
            |at| {
                reads += 1;
                served(at)
            },
            String::as_str,
        )
        .unwrap();
        (items, cursor, reads)
    }

    #[test]
    fn a_reread_pages_as_far_as_was_shown_and_no_further() {
        assert_eq!(
            filled(2),
            (vec!["1".into(), "2".into()], Some("c-1".into()), 0)
        );
        let (items, cursor, reads) = filled(4);
        assert_eq!((items.len(), cursor.as_deref(), reads), (4, Some("c-2"), 1));
        let (items, cursor, reads) = filled(9);
        assert_eq!((items.len(), cursor, reads), (5, None, 2), "cursor ran out");
    }

    #[test]
    fn a_failed_page_fails_the_reread() {
        let mut items = vec!["1".to_owned()];
        let mut cursor = Some("gone".to_owned());
        let extent = Extent { rows: 3, size: 2 };
        assert!(fill(&mut items, &mut cursor, extent, served, String::as_str).is_err());
    }
}
