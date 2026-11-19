//! The optional system terminal profile fallback.

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub(super) use macos::{load, register};

#[cfg(not(target_os = "macos"))]
pub(super) fn load() -> crate::style::ProfileFont {
    crate::style::ProfileFont::default()
}

#[cfg(not(target_os = "macos"))]
pub(super) fn register() {}
