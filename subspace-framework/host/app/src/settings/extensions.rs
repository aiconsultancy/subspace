//! Product-composed Settings pages. Each retained view owns its operations.

use gpui_kit::{AnyView, App, Global, Window};
use subspace_lucide::IconName;

/// A product capability's Application destination in Settings.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct SettingsExtension {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: IconName,
    pub create: fn(&mut Window, &mut App) -> AnyView,
    github_credentials: bool,
}

impl SettingsExtension {
    pub const fn new(
        id: &'static str,
        title: &'static str,
        icon: IconName,
        create: fn(&mut Window, &mut App) -> AnyView,
    ) -> Self {
        Self {
            id,
            title,
            icon,
            create,
            github_credentials: false,
        }
    }

    /// Compose the shared GitHub account and token draft above this view.
    // qual:api
    pub const fn with_github_credentials(mut self) -> Self {
        self.github_credentials = true;
        self
    }

    pub const fn has_github_credentials(&self) -> bool {
        self.github_credentials
    }
}

pub(crate) struct SettingsExtensions(pub &'static [SettingsExtension]);
impl Global for SettingsExtensions {}

pub(super) fn registrations(cx: &App) -> &'static [SettingsExtension] {
    cx.try_global::<SettingsExtensions>()
        .map_or(&[], |extensions| extensions.0)
}

pub(super) fn create_views(window: &mut Window, cx: &mut App) -> Vec<(&'static str, AnyView)> {
    registrations(cx)
        .iter()
        .map(|extension| (extension.id, (extension.create)(window, cx)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{SettingsExtension, SettingsExtensions};
    use gpui_kit::{
        AnyView, App, AppContext as _, Context, Global, IntoElement, Render, TestAppContext,
        Window, div,
    };
    struct Extension;
    struct Creates(usize);
    impl Global for Creates {}
    impl Render for Extension {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }
    fn create(_: &mut Window, cx: &mut App) -> AnyView {
        cx.global_mut::<Creates>().0 += 1;
        cx.new(|_| Extension).into()
    }
    const EXTENSIONS: &[SettingsExtension] = &[SettingsExtension::new(
        "fixture",
        "Fixture",
        subspace_lucide::IconName::Settings,
        create,
    )];

    #[test]
    fn github_credentials_require_explicit_extension_opt_in() {
        assert!(!EXTENSIONS[0].has_github_credentials());
        assert!(
            EXTENSIONS[0]
                .with_github_credentials()
                .has_github_credentials()
        );
    }

    #[gpui_kit::test]
    fn application_extensions_are_retained_and_absent_products_need_no_extension(
        cx: &mut TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            cx.set_global(Creates(0));
            cx.set_global(SettingsExtensions(EXTENSIONS));
        });
        let settings = crate::settings::preview_tests::open_settings(directory.path(), cx);
        settings
            .update(cx, |settings, window, cx| {
                assert_eq!(cx.global::<Creates>().0, 1);
                assert!(
                    settings
                        .navigation
                        .contains(&crate::settings::SettingsPage::Extension("fixture"))
                );
                let view = settings.extensions[0].1.clone();
                settings.reconcile_boards(window, cx);
                assert_eq!(settings.extensions[0].1.entity_id(), view.entity_id());
                assert_eq!(cx.global::<Creates>().0, 1);
                settings.restore_preview(window, cx).unwrap();
            })
            .unwrap();
        cx.update(|cx| {
            cx.set_global(SettingsExtensions(&[]));
            assert!(super::registrations(cx).is_empty());
        });
    }
}
