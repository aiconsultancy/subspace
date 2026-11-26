//! GitHub's GraphQL and REST shapes and their mapping to the contract types.

mod activity;
mod projection;
mod types;

pub use activity::{to_detail, viewer_approved};
pub use projection::{assignable_users, extras, section_items, to_diff_file};
pub use types::{Nodes, RawActor, RawDetail, RawPrFile};

#[cfg(test)]
mod tests;
