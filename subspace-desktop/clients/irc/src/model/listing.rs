//! A channel the server lists.

use serde::{Deserialize, Serialize};

/// One channel from the server's `LIST` reply (`RPL_LIST`, 322).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListedChannel {
    pub name: String,
    /// The visible users the server counted.
    pub users: u32,
    /// The topic as sent, mIRC formatting intact; some servers lead it with
    /// the channel modes, such as `[+nt]`.
    #[serde(default)]
    pub topic: String,
}
