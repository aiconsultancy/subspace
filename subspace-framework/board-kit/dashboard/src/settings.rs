//! Static settings metadata supplied by a dashboard registration, and the
//! run-time fields a configured board can add.

mod connection;
mod editor;
mod fields;

use subspace_core::features::FeatureSpec;
use subspace_lucide::IconName;

pub use connection::{ConnectionField, ConnectionSettings, ConnectionToken};
pub use editor::{BoardSettingsEditor, SettingsEditor, SettingsEditorFactory};
pub use fields::{BoardFields, FieldKind, FieldSpec, MAX_FIELD_TEXT_CHARS, PermissionSpec};

/// Settings owned by one board. The metadata is static, so registration can
/// inspect it without constructing the dashboard or starting service work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoardSettings {
    pub icon: Option<IconName>,
    /// Shown under the breadcrumb on the board's Settings page. Empty means no
    /// board-specific description. Supports Markdown, including inline code
    /// and external links, rendered as a wrapping paragraph.
    pub description: &'static str,
    pub sections: &'static [SettingsSection],
    pub features: &'static [FeatureSpec],
    /// IDs from `features` that the board reads dynamically and applies on Save.
    /// All other features require a restart.
    pub live_features: &'static [&'static str],
    /// Opt in to generic notification controls, labelled with the board title.
    pub notifications: bool,
    /// The notification choices that apply until the person saves their own.
    /// Off unless the board declares otherwise.
    pub notification_defaults: crate::notifications::Preferences,
    /// Opt in to per-board automatic network refresh and polling controls.
    pub background_updates: bool,
    /// A searchable board's store starts on: the Search view's caching
    /// switch, which keeps the board's data on this computer, is on until the
    /// person chooses. Off, the default, means the person opts in.
    pub store_by_default: bool,
}

/// One recursively nestable page below a board's main settings page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingsSection {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: IconName,
    pub children: &'static [SettingsSection],
    pub features: &'static [FeatureSpec],
}
