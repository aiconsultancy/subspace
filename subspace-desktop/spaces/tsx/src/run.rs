//! Runs one verified package in an nsh `AppView`: the grant set on its
//! bundle, the host channel answering the granted tiers, the Subspace
//! theme, and fatal errors reported back to the Space as they happen.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Once;

use gpui_kit::{App, AppContext as _, Context, Entity, Window};

use crate::TsxSpaces;
use nsh_engine::{App as NshApp, HostChannel};
use nsh_platform_gpui::{AppOptions, AppView, Appearance, AssetBase};
use serde_json::json;
use subspace_framework::dashboard::{Tokens, workspace};
use subspace_tsx_system::Tier;

use crate::host::{self, Watch};
use crate::store::{Paths, Verified};

/// A package running in the content pane.
pub(crate) struct Running {
    /// The package hash it runs.
    pub(crate) sha256: String,
    pub(crate) view: Entity<AppView>,
    pub(crate) granted: Vec<Tier>,
    channel: HostChannel,
    /// The last fatal error, which stops the package.
    pub(crate) failure: Rc<RefCell<Option<String>>>,
    watch: Option<Watch>,
}

static UI_SETS: Once = Once::new();

/// Starts `verified` with only `granted`.
pub(crate) fn start(
    verified: &Verified,
    granted: Vec<Tier>,
    paths: &Paths,
    window: &mut Window,
    cx: &mut Context<TsxSpaces>,
) -> Result<Running, String> {
    UI_SETS.call_once(|| nsh_platform_gpui::set_ui_sets(vec![nsh_base::registry()]));
    let bundle = granted_bundle(verified, &granted)?;
    let channel = HostChannel::default();
    host::hub(cx).serve(&channel, &granted);
    let options = options(verified, paths, &channel);
    let view = cx.new(|cx| AppView::new(options, window, cx));
    let failure = Rc::new(RefCell::new(None));
    let slot = failure.clone();
    let space = cx.entity().downgrade();
    let async_cx = cx.to_async();
    let assets = verified.assets.clone();
    view.update(cx, |view, _| {
        view.on_app_error(move |error| {
            if error.is_fatal() {
                *slot.borrow_mut() = Some(error.to_string());
                // Redraw the Space once the engine's callback has returned.
                let space = space.clone();
                async_cx
                    .spawn(async move |cx| space.update(cx, |_, cx| cx.notify()).ok())
                    .detach();
            }
        });
        view.set_asset_base(Some(AssetBase::Directory(assets)));
        view.load(NshApp::Js(bundle));
    });
    let running = Running {
        sha256: verified.sha256.clone(),
        view,
        granted,
        channel,
        failure,
        watch: None,
    };
    apply_theme(&running, cx);
    Ok(running)
}

/// An embedded view of the package: its own storage folder, this host
/// channel, the default limits.
// qual:allow(dry, boilerplate) reason: "nsh's options own their strings; the copies are three short values per start."
fn options(verified: &Verified, paths: &Paths, channel: &HostChannel) -> AppOptions {
    let app_id = &verified.identity.app_id;
    AppOptions {
        inline_root: true,
        app_id: app_id.clone(),
        title: verified.name.clone(),
        data_dir: Some(paths.data(app_id, &verified.identity.key_id)),
        host: channel.clone(),
        ..AppOptions::default()
    }
}

/// The package's bundle with an explicit grant: the granted host names and
/// nothing else, so a package without a declaration gets no more.
fn granted_bundle(verified: &Verified, granted: &[Tier]) -> Result<nsh_engine::Bundle, String> {
    let names: Vec<&str> = granted.iter().map(|tier| tier.host_name()).collect();
    let capabilities = nsh_engine::parse_capabilities(&json!({ "host": names }).to_string())?;
    let mut bundle = verified.bundle.clone();
    bundle.policy.granted = Some(capabilities);
    Ok(bundle)
}

impl Running {
    /// Sends new readings while the Space is visible; stops when it is not.
    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut App) {
        self.watch = visible.then(|| host::hub(cx).watch(&self.channel, &self.granted));
    }

    /// Stops the package's engine and its readings.
    pub(crate) fn stop(mut self, cx: &mut App) {
        self.watch = None;
        self.view.update(cx, |view, _| view.stop());
    }
}

/// Hands the app's theme to the package: its tokens, the text edge, and
/// light or dark.
pub(crate) fn apply_theme(running: &Running, cx: &mut App) {
    let tokens = Tokens::current(cx);
    let dark = gpui_kit::component::Theme::global(cx).is_dark();
    let vars = crate::theme::css_vars(&tokens, workspace::text_edge(&tokens)).to_string();
    running.view.update(cx, |view, cx| {
        // A theme the package cannot read leaves its own in place.
        let _ = view.set_theme_json(&vars, cx);
        view.set_appearance(
            if dark {
                Appearance::Dark
            } else {
                Appearance::Light
            },
            cx,
        );
    });
}

/// Hands every theme change to the running package.
pub(crate) fn follow_theme(cx: &mut Context<TsxSpaces>) {
    cx.observe_global::<gpui_kit::component::Theme>(|this, cx| {
        if let Some(running) = &this.running {
            apply_theme(running, cx);
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::granted_bundle;
    use crate::fixtures::{package, signer};
    use crate::store::{Paths, install};
    use subspace_tsx_system::Tier;

    #[test]
    fn the_bundle_is_granted_exactly_the_granted_names() {
        let folder = tempfile::tempdir().unwrap();
        let paths = Paths::beside(&folder.path().join("config.json"));
        let signer = signer();
        let source = folder.path().join("app.nshpkg");
        std::fs::write(&source, package(&signer, "dev.example.app", 1, false)).unwrap();
        let verified = install(&paths, std::slice::from_ref(&signer.root), &source).unwrap();
        let none = granted_bundle(&verified, &[]).unwrap();
        assert!(none.policy.granted.unwrap().host.is_empty());
        let some = granted_bundle(&verified, &[Tier::Metrics, Tier::Facts]).unwrap();
        assert_eq!(
            some.policy.granted.unwrap().host,
            ["subspace.system.metrics", "subspace.system.facts"]
        );
    }
}
