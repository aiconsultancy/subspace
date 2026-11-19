//! Board-owned embedded assets, independent of the product's switcher order.

/// Artwork declared by a board and captured by its registration.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoardArtwork {
    entries: &'static [(&'static str, &'static [u8])],
    order: u16,
}

impl BoardArtwork {
    pub const fn new(entries: &'static [(&'static str, &'static [u8])]) -> Self {
        Self { entries, order: 0 }
    }

    /// Preserve asset catalog precedence independently of switcher ordering.
    /// Lower values come first; ties retain registration order. Products' own
    /// artwork always precedes boards, and GPUI Kit remains the fallback.
    pub const fn with_order(mut self, order: u16) -> Self {
        self.order = order;
        self
    }

    pub fn entries(&self) -> &'static [(&'static str, &'static [u8])] {
        self.entries
    }

    pub fn order(&self) -> u16 {
        self.order
    }
}

#[cfg(test)]
mod tests {
    use super::BoardArtwork;

    #[test]
    fn declarations_keep_bytes_keys_and_explicit_catalog_order() {
        const ENTRIES: &[(&str, &[u8])] = &[("fixture.svg", b"fixture")];
        const ARTWORK: BoardArtwork = BoardArtwork::new(ENTRIES).with_order(7);
        assert_eq!(ARTWORK.entries(), ENTRIES);
        assert_eq!(ARTWORK.order(), 7);
        assert!(BoardArtwork::default().entries().is_empty());
        assert_eq!(BoardArtwork::new(ENTRIES).order(), 0);
    }
}
