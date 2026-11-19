//! Transient surfaces close when the chat owner is hidden or deactivated.

use super::{ChatPanel, Context, Window};

impl ChatPanel {
    /// Dismiss toolbar surfaces before the panel is hidden or deactivated.
    pub fn dismiss_popups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.toolbar
            .update(cx, |toolbar, cx| toolbar.dismiss_popups(window, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::ChatPanel;
    use crate::prefs;
    use gpui_kit::base::GlobalState;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};
    use subspace_agent::{ChatEvent, Command, Event, SessionSummary};
    use subspace_core::config::ChatEngine;

    fn setup(cx: &mut TestAppContext) -> (Entity<ChatPanel>, &mut VisualTestContext) {
        setup_engine(cx, ChatEngine::Copilot)
    }

    fn setup_engine(
        cx: &mut TestAppContext,
        engine: ChatEngine,
    ) -> (Entity<ChatPanel>, &mut VisualTestContext) {
        cx.update(|cx| {
            cx.set_reduce_motion(true);
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
        });
        let (chat, cx) =
            cx.add_window_view(move |window, cx| ChatPanel::with_engine(engine, window, cx));
        settle(cx);
        (chat, cx)
    }

    #[gpui_kit::test]
    fn opening_chat_while_off_shows_setup_guidance_without_starting_a_runtime(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = setup_engine(cx, ChatEngine::default());
        cx.update(|window, cx| {
            chat.update(cx, |chat, cx| {
                chat.on_open(window, cx);
                assert!(chat.is_disabled());
                assert!(chat.handle.is_none());
                assert!(!chat.is_unavailable());
            });
        });
        settle(cx);
        cx.update(|window, _| {
            assert!(window.find("chat-off").visible());
            assert!(window.try_find("chat-new-tab").is_none());
            assert!(window.try_find("chat-reconnect").is_none());
        });
    }

    fn settle(cx: &mut VisualTestContext) {
        for _ in 0..3 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
    }

    #[gpui_kit::test]
    fn sessions_dismisses_retained_and_keyed_toolbar_hosts_before_composer_reopens(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = setup(cx);
        for (trigger, body) in [
            ("chat-usage", "chat-usage-body"),
            ("chat-add", "popup-menu"),
        ] {
            cx.update(|window, cx| window.click(trigger, cx));
            settle(cx);
            cx.update(|window, cx| {
                assert!(window.find(body).visible());
                assert!(GlobalState::is_in_deferred_context(cx));
                chat.update(cx, |chat, cx| chat.show_sessions(window, cx));
            });
            settle(cx);
            cx.update(|window, cx| {
                assert!(window.try_find(trigger).is_none());
                assert!(!GlobalState::is_in_deferred_context(cx));
                chat.update(cx, |chat, cx| chat.focus_chat(0, window, cx));
            });
            settle(cx);
            cx.update(|window, _| {
                assert!(window.find(trigger).visible());
                assert!(window.try_find(body).is_none());
            });
        }
        cx.update(|window, cx| window.click("chat-add", cx));
        settle(cx);
        cx.update(|window, _| assert!(window.find("popup-menu").visible()));
    }

    #[gpui_kit::test]
    fn reopen_retains_runtime_and_reconnect_shuts_it_down(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let (handle, mut commands) =
            subspace_agent::AgentHandle::test_offline(directory.path().into(), Vec::new());
        let original = handle.clone();
        let (chat, cx) = setup(cx);
        cx.update(|window, cx| {
            chat.update(cx, |chat, cx| {
                chat.connected(Ok(handle));
                chat.on_open(window, cx);
                chat.dismiss_popups(window, cx);
                chat.on_open(window, cx);
                assert!(chat.handle.as_ref().unwrap().same_runtime(&original));
                assert!(
                    commands.try_recv().is_err(),
                    "hide/reopen must not restart the runtime"
                );
                chat.reconnect(window, cx);
                assert!(matches!(commands.try_recv(), Ok(Command::Shutdown)));
                assert!(commands.try_recv().is_err());
                assert!(chat.handle.is_none());
                assert_eq!(
                    chat.error.as_deref(),
                    Some("Service configuration is unavailable.")
                );
            })
        });
    }

    #[gpui_kit::test]
    fn startup_rejection_reaches_panel_polling_and_reconnect_clears_sign_in(
        cx: &mut TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let rejection = "GitHub rejected the gh CLI token for test-user. Click Sign in to GitHub.";
        let (handle, mut commands) = subspace_agent::AgentHandle::test_offline(
            directory.path().into(),
            vec![
                Event::SignInRequired(subspace_agent::SignInOffer::GhRefresh {
                    login: "test-user".into(),
                }),
                Event::Error(rejection.into()),
            ],
        );
        let (chat, cx) = setup(cx);
        cx.update(|window, cx| chat.update(cx, |chat, cx| {
            chat.connected(Ok(handle));
            chat.poll(cx);
            assert_eq!(chat.error.as_deref(), Some(rejection));
            assert!(matches!(&chat.sign_in, Some(super::super::sign_in::SignIn::Offered { offer: subspace_agent::SignInOffer::GhRefresh { login }, .. }) if login == "test-user"));
            assert!(!chat.tab().is_ready());
            chat.reconnect(window, cx);
            assert!(matches!(commands.try_recv(), Ok(Command::Shutdown)));
            assert!(chat.sign_in.is_none());
        }));
    }

    #[gpui_kit::test]
    fn restored_runtime_events_publish_saved_sessions_with_tab_activity(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let events = vec![
            Event::of(2, ChatEvent::History(Vec::new())),
            Event::of(2, ChatEvent::Title("Restored second".into())),
            Event::of(
                2,
                ChatEvent::Ready {
                    session_id: "saved-second".into(),
                },
            ),
            Event::of(
                1,
                ChatEvent::Ready {
                    session_id: "saved-first".into(),
                },
            ),
            Event::Restore {
                tabs: vec![(1, "saved-first".into()), (2, "saved-second".into())],
                active: 2,
            },
            Event::Sessions(vec![SessionSummary {
                id: "saved-second".into(),
                title: "Saved second".into(),
                modified: "now".into(),
                modified_at: "2026-11-04T00:00:00Z".into(),
                created_at: "2026-11-03T00:00:00Z".into(),
            }]),
        ];
        let (handle, mut commands) =
            subspace_agent::AgentHandle::test_offline(directory.path().into(), events);
        let (chat, cx) = setup(cx);
        cx.update(|_, cx| {
            chat.update(cx, |chat, cx| {
                chat.connected(Ok(handle));
                chat.poll(cx);
                assert_eq!(
                    chat.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
                    [1, 2]
                );
                assert_eq!(chat.tab().title.as_ref(), "Restored second");
                assert_eq!(chat.tab().session.as_deref(), Some("saved-second"));
                assert!(matches!(commands.try_recv(), Ok(Command::ListSessions)));
                chat.tab_mut().begin();
                chat.publish_sessions(cx);
                let sessions = crate::shared::sessions(cx);
                assert_eq!(sessions.len(), 1);
                assert!(sessions[0].open && sessions[0].busy);
                assert_eq!(sessions[0].id, "saved-second");
            })
        });
    }

    #[gpui_kit::test]
    fn application_quit_sends_shutdown_to_the_retained_runtime(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let (handle, mut commands) =
            subspace_agent::AgentHandle::test_offline(directory.path().into(), Vec::new());
        cx.update(|cx| {
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
        });
        let window = cx.add_window(ChatPanel::new);
        window
            .update(cx, |chat, _, _| chat.connected(Ok(handle)))
            .unwrap();
        cx.quit();
        assert!(matches!(commands.try_recv(), Ok(Command::Shutdown)));
        assert!(commands.try_recv().is_err());
    }

    /// An error the runtime reports during a turn leaves it running, so the
    /// next message steers it rather than starting a new turn.
    #[gpui_kit::test]
    fn a_message_after_an_error_during_a_turn_steers_it(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let (handle, _commands) =
            subspace_agent::AgentHandle::test_offline(directory.path().into(), Vec::new());
        let (chat, cx) = setup(cx);
        chat.update(cx, |chat, cx| {
            chat.connected(Ok(handle));
            chat.tab_mut().ready();
            let mut first = chat.prepare_submission(
                super::super::InputIntent::Message("Fix it".into()),
                Vec::new(),
            );
            let command = first.command.take().expect("a send");
            let sent = chat.send(command, cx);
            chat.finish_submission(first, sent);
            let running = chat.tab().running().copied();
            super::super::attention::apply_chat_event(
                chat.tab_mut(),
                ChatEvent::Error("refused".into()),
            );
            let steer = chat.prepare_submission(
                super::super::InputIntent::Message("Also".into()),
                Vec::new(),
            );
            assert!(steer.steering, "the turn is still running");
            assert!(steer.last_turn.is_none(), "Retry keeps the first message");
            assert_eq!(chat.tab().running().copied(), running, "and its timer");
        });
    }
}
