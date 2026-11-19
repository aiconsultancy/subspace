//! GPUI Kit components, one card each, grouped as GPUI Kit's documentation
//! groups them: basic, form, layout and advanced components. Each sample
//! uses the component as it ships, themed by the active theme, except where
//! Subspace routes it through a shared boundary (popups through `anchored`,
//! switches through `toggle::switch`); the card names that treatment.

pub(crate) mod advanced;
pub(crate) mod basic;
pub(crate) mod form;
pub(crate) mod layout;

/// The source file of a GPUI Kit component, as cards show it.
macro_rules! kit_source {
    ($file:literal) => {
        concat!("gpui-component 0.9.1: src/", $file)
    };
}
pub(crate) use kit_source;
