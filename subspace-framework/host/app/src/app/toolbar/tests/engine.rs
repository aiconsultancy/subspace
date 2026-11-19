//! The selected chat engine's mark reaches the Chat button and the status bar.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, px};
use subspace_agent_tools::Catalog;
use subspace_core::config::{AcpAgent, AcpConfig, ChatConfig, ChatEngine, Config};
use subspace_core::credentials::Credentials;
use subspace_dashboard::Services;

use super::status::rooted_fixture;

fn select_codex(cx: &mut TestAppContext, root: &std::path::Path) {
    let config = Config {
        chat: ChatConfig {
            engine: ChatEngine::Acp,
            acp: AcpConfig {
                agent: AcpAgent::Codex,
                ..AcpConfig::default()
            },
        },
        ..Config::default()
    };
    let config_file = root.join("config.json");
    cx.update(|cx| {
        cx.set_global(Services {
            credentials: Arc::new(Credentials::from_system(&config)),
            config: Arc::new(config),
            tools: Catalog::new(Vec::new()).unwrap(),
            search: None,
            config_file,
        });
    });
}

#[gpui_kit::test]
fn acp_codex_is_shown_by_name_with_the_openai_mark(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    select_codex(cx, root.path());
    let (window, app) = rooted_fixture(cx, root.path(), 2200., false);
    let (facts, button_icon) = app.read_with(cx, |app, cx| {
        (app.status_facts(cx), app.chat.read(cx).engine_icon())
    });
    assert_eq!(facts.chat_engine, "Codex");
    let mark = std::str::from_utf8(facts.chat_icon.bytes()).unwrap();
    assert!(mark.contains("OpenAI mark"), "{mark}");
    assert_eq!(button_icon, facts.chat_icon);
    cx.update_window(window, |_, window, _| {
        let status = window.find("status-chat").bounds();
        let glyph = window
            .within("status-chat")
            .find("status-chat-mark")
            .bounds();
        let help = window.within("status-help").find("toolbar-icon").bounds();
        assert_eq!(glyph.size, help.size, "the mark matches the status glyphs");
        assert_eq!(glyph.left(), status.left());
        assert!((glyph.center().y - status.center().y).abs() <= px(1.));
        assert!(
            window
                .within("toggle-chat")
                .try_find("toolbar-icon")
                .is_some()
        );
    })
    .unwrap();
}
