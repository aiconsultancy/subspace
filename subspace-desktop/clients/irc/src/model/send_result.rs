//! How a `SendAll` ended.

/// How a [`Command::SendAll`] ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendResult {
    /// Every line was written to the connection. That is not the server's
    /// acceptance: it can still drop or refuse a line.
    Written,
    /// Nothing was written, for this reason.
    Refused(String),
    /// The connection failed while writing: some lines may have gone.
    Uncertain(String),
}

impl SendResult {
    /// Why the lines may not all have gone, unless they were written.
    pub fn problem(&self) -> Option<&str> {
        match self {
            Self::Written => None,
            Self::Refused(why) | Self::Uncertain(why) => Some(why),
        }
    }
}
