//! A write that did not land: its plain words, and whether it may have
//! landed anyway because the tracker never confirmed it.

/// Why a write did not land.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Failed {
    pub words: String,
    /// The request was sent but its outcome is unknown, so the same write
    /// must not be sent again from where it came.
    pub unconfirmed: bool,
}

impl Failed {
    /// A write whose outcome the tracker never confirmed.
    pub(crate) fn unconfirmed(words: String) -> Self {
        Self {
            words,
            unconfirmed: true,
        }
    }
}

impl From<String> for Failed {
    fn from(words: String) -> Self {
        Self {
            words,
            unconfirmed: false,
        }
    }
}
