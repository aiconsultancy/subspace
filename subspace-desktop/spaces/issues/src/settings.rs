//! Settings › Board › Issues shows the connection without credential inputs.
mod hub;
mod view;
use crate::scope::Access;
use gpui_kit::{App, AppContext as _, Context, Task, Window};
use serde_json::{Map, Value};
use subspace_api_client::hub::Hub;
use subspace_framework::dashboard::settings::{BoardSettingsEditor, SettingsEditor};

pub(crate) const CONNECTION_NOTE: &str = "Sign in under Settings › Account.";

fn api_address() -> String {
    subspace_api_client::configured_api_origin()
        .unwrap_or("No Subspace API configured")
        .into()
}
pub(crate) struct IssuesSettings {
    access: Access,
    address: String,
    hub: Option<Hub>,
    hub_id: Option<u64>,
    seen: Option<u64>,
    _poll: Option<Task<()>>,
}
pub(crate) fn factory(
    _saved: Map<String, Value>,
    _enabled: bool,
    _window: &mut Window,
    cx: &mut App,
) -> SettingsEditor {
    let editor = cx.new(|cx| {
        let mut editor = IssuesSettings::new();
        editor.poll_hub(cx);
        editor.start_polling(cx);
        editor
    });
    SettingsEditor::new(editor)
}
impl IssuesSettings {
    pub(crate) fn new() -> Self {
        Self {
            access: Access::Starting,
            address: api_address(),
            hub: None,
            hub_id: None,
            seen: None,
            _poll: None,
        }
    }
    fn connection_state(&self) -> String {
        match &self.access {
            Access::Ready { display_name } => format!("Signed in as {display_name}"),
            Access::Starting => {
                "Connecting… The first load can take up to a minute while Subspace API wakes."
                    .into()
            }
            Access::Failed(crate::listing::Failure::Edge) => "Sign-in required".into(),
            Access::Failed(_) => "Offline".into(),
        }
    }
}
impl BoardSettingsEditor for IssuesSettings {
    fn snapshot(&self, _cx: &App) -> Value {
        Value::Null
    }
    fn values(&self, _cx: &App) -> Result<Map<String, Value>, String> {
        Ok(Map::new())
    }
    fn set_enabled(&mut self, _enabled: bool, _cx: &mut Context<Self>) {}
}
#[cfg(test)]
#[path = "tests/settings.rs"]
mod tests;
