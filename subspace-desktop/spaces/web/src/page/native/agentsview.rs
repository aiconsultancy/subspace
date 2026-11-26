//! An isolated, scoped view of the fleet's AgentsView installation.

use std::cell::Cell;

use gpui_kit::{Context, Window};
#[cfg(windows)]
use subspace_framework::dashboard::Dashboard as _;

use super::{Admission, Page, PageEvent, View, capture_visible, channel};
use crate::agentsview::{Connection, ConnectionEvent};

/// AgentsView routes within its document. A full reload uses a fresh,
/// hidden view so only its own settings response can make it visible.
fn first_document(home: String) -> impl Fn(&str) -> Admission {
    let started = Cell::new(false);
    move |target| {
        if target == home && !started.replace(true) {
            Admission::Load
        } else {
            Admission::Refuse
        }
    }
}

impl Page {
    pub(crate) fn open_agentsview<T: 'static>(
        connection: &Connection,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Result<Self, &'static str> {
        let address = &connection.address;
        let config = serde_json::json!({
            "origin": address.origin().ascii_serialization(),
            "apiPath": format!("{}api/", address.path()),
            "key": connection.key.as_ref().map(|key| key.expose()),
        });
        let script = include_str!("../../agentsview/auth.js")
            .replace("/*__SUBSPACE_AGENTSVIEW_CONFIG__*/", &config.to_string());
        let (events, inbox) = channel();
        let reporter = events.clone();
        let home = address.clone();
        #[cfg(target_os = "macos")]
        let (builder, store) = (wry::WebViewBuilder::new(), ());
        #[cfg(windows)]
        let mut store = super::web_context(&crate::page::Profile::for_board(
            crate::agentsview::AgentsView::ID,
            cx,
        ))
        .map_err(|_| "Subspace could not create its web view profile.")?;
        #[cfg(windows)]
        let builder = wry::WebViewBuilder::new_with_web_context(&mut store);
        let builder = builder
            .with_incognito(true)
            .with_initialization_script_for_main_only(script, true)
            .with_ipc_handler(move |request| {
                if Connection::admits(&home, &request.uri().to_string())
                    && let Some(event) = ConnectionEvent::parse(request.body())
                {
                    reporter.send(PageEvent::AgentsView(event));
                }
            });
        let view = View {
            builder: capture_visible(builder).with_visible(false),
            url: address.as_str(),
            admit: Box::new(first_document(address.to_string())),
            app: None,
            devtools: false,
        };
        let raw = view
            .build(events, window)
            .map_err(|_| "Subspace could not open its web view.")?;

        let page = Self::adopt(raw, inbox, store, window, cx);
        page.set_shown(false, cx);
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::{Admission, first_document};

    #[test]
    fn a_view_only_loads_its_first_document() {
        let home = "https://fleet.example.com/agentsview/";
        let admit = first_document(home.into());
        assert_eq!(admit("https://elsewhere.example.com/"), Admission::Refuse);
        assert_eq!(admit(home), Admission::Load);
        assert_eq!(admit(home), Admission::Refuse);
        assert_eq!(
            admit("https://fleet.example.com/agentsview/sessions"),
            Admission::Refuse
        );
        assert_eq!(first_document(home.into())(home), Admission::Load);
    }
}
