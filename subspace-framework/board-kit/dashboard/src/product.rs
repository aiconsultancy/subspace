//! The current product's immutable configuration context, shared with boards.

use std::sync::Arc;

use gpui_kit::Global;
use subspace_product::ProductContext;

/// Set by the product shell before constructing dashboard entities. Boards use
/// the same context for their additional configuration directories.
#[derive(Clone)]
pub struct CurrentProduct(pub Arc<ProductContext>);

impl Global for CurrentProduct {}
