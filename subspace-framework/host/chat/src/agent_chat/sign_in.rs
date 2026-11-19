//! Signing chat's GitHub account in after it was missing or rejected. An
//! account Subspace signed in, or none at all, signs in with GitHub in
//! Subspace (the shared [`github_sign_in`]); gh's active account signs in
//! again with `gh auth refresh`, which prints a one-time code that the panel
//! copies before it opens GitHub's device page. Either way the panel
//! reconnects when the sign-in finishes.

use gpui_kit::{AnyElement, App, AppContext as _};
use subspace_agent::SignInOffer;
use subspace_core::credentials::gh_sign_in::{DEVICE_URL, GhSignIn};
use subspace_dashboard::Tokens;
use subspace_dashboard::github_sign_in::{self, SignInEvent};

use super::{ChatPanel, ClipboardItem, Context, services};

/// Where the sign-in stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SignIn {
    /// The runtime offered `offer`. `failure` is why the last gh attempt did
    /// not finish; the shared sign-in shows its own.
    Offered {
        offer: SignInOffer,
        failure: Option<String>,
    },
    /// gh is starting and has not printed its code yet.
    Starting { login: String },
    /// gh waits for the user to enter `code` on GitHub.
    Waiting { login: String, code: String },
}

impl SignIn {
    pub(super) fn offered(offer: SignInOffer) -> Self {
        Self::Offered {
            offer,
            failure: None,
        }
    }

    /// Whether this is a sign-in with GitHub in Subspace.
    pub(super) fn in_app(&self) -> bool {
        matches!(
            self,
            Self::Offered {
                offer: SignInOffer::InApp { .. },
                ..
            }
        )
    }

    /// Whether gh is running for this sign-in.
    pub(super) fn in_progress(&self) -> bool {
        !matches!(self, Self::Offered { .. })
    }

    /// The line under the error: gh's code to enter, or why gh failed.
    pub(super) fn note(&self) -> Option<String> {
        match self {
            Self::Offered { failure, .. } => failure.clone(),
            Self::Starting { .. } => Some("Starting GitHub sign-in…".into()),
            Self::Waiting { code, .. } => Some(format!(
                "Enter code {code} at {DEVICE_URL}. The code is on the clipboard. Subspace \
                 reconnects when GitHub confirms the sign-in."
            )),
        }
    }
}

/// Redraws the panel as the shared sign-in moves on, and reconnects after it.
pub(super) fn subscriptions(cx: &mut Context<ChatPanel>) -> Vec<gpui_kit::Subscription> {
    github_sign_in::shared(cx)
        .map(|sign_in| {
            vec![
                cx.observe(&sign_in, |_, _, cx| cx.notify()),
                cx.subscribe(&sign_in, |chat, _, event, cx| {
                    chat.on_sign_in_event(event, cx)
                }),
            ]
        })
        .unwrap_or_default()
}

impl ChatPanel {
    /// Starts the offered sign-in.
    pub(super) fn sign_in(&mut self, cx: &mut Context<Self>) {
        match &self.sign_in {
            Some(SignIn::Offered {
                offer: SignInOffer::InApp { .. },
                ..
            }) => {
                if let Some(shared) = github_sign_in::shared(cx) {
                    shared.update(cx, |sign_in, cx| sign_in.start(cx));
                }
            }
            Some(SignIn::Offered {
                offer: SignInOffer::GhRefresh { login },
                ..
            }) => {
                let login = login.clone();
                self.gh_sign_in(login, cx);
            }
            _ => {}
        }
    }

    /// Whether a sign-in for this panel is running.
    pub(super) fn signing_in(&self, cx: &App) -> bool {
        let Some(sign_in) = &self.sign_in else {
            return false;
        };
        sign_in.in_progress()
            || (sign_in.in_app()
                && github_sign_in::shared(cx).is_some_and(|shared| shared.read(cx).in_progress()))
    }

    /// The shared sign-in's progress, while this panel offers it.
    pub(super) fn in_app_status(
        &self,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let shared = github_sign_in::shared(cx)
            .filter(|_| self.sign_in.as_ref().is_some_and(SignIn::in_app))?;
        let phase = shared.read(cx).phase().clone();
        let cancel = move |_: &gpui_kit::ClickEvent, _: &mut gpui_kit::Window, cx: &mut App| {
            shared.update(cx, |sign_in, cx| sign_in.cancel(cx));
        };
        github_sign_in::status("chat-sign-in", &phase, tokens, cancel, cx)
    }

    /// Reconnects once the offered sign-in in Subspace finishes.
    fn on_sign_in_event(&mut self, event: &SignInEvent, cx: &mut Context<Self>) {
        let offered = self.sign_in.as_ref().is_some_and(SignIn::in_app);
        if offered && matches!(event, SignInEvent::SignedIn(_)) {
            self.restart_runtime(cx);
        }
    }

    /// Runs `gh auth refresh` off the UI thread and reconnects when it
    /// succeeds.
    fn gh_sign_in(&mut self, login: String, cx: &mut Context<Self>) {
        self.sign_in = Some(SignIn::Starting {
            login: login.clone(),
        });
        cx.notify();
        cx.spawn(async move |chat, cx| {
            let started = cx.background_spawn(async { GhSignIn::start() }).await;
            let signing = match started {
                Ok(signing) => signing,
                Err(error) => {
                    let _ = chat.update(cx, |chat, cx| chat.sign_in_failed(login, error.0, cx));
                    return;
                }
            };
            let code = signing.code().to_owned();
            let _ = chat.update(cx, |chat, cx| chat.show_code(login.clone(), code, cx));
            let finished = cx.background_spawn(async move { signing.finish() }).await;
            let _ = chat.update(cx, |chat, cx| match finished {
                Ok(()) => chat.signed_in(&login, cx),
                Err(error) => chat.sign_in_failed(login, error.0, cx),
            });
        })
        .detach();
    }

    fn show_code(&mut self, login: String, code: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
        crate::links::open(DEVICE_URL, cx);
        self.sign_in = Some(SignIn::Waiting { login, code });
        cx.notify();
    }

    fn sign_in_failed(&mut self, login: String, failure: String, cx: &mut Context<Self>) {
        self.sign_in = Some(SignIn::Offered {
            offer: SignInOffer::GhRefresh { login },
            failure: Some(failure),
        });
        cx.notify();
    }

    /// gh holds the new token now; the cached one is the rejected token.
    fn signed_in(&mut self, login: &str, cx: &mut Context<Self>) {
        if let Some(services) = services(cx) {
            services.credentials.forget_gh_token(login);
        }
        self.restart_runtime(cx);
    }
}

#[cfg(test)]
mod tests {
    use subspace_agent::SignInOffer;

    use super::SignIn;

    #[test]
    fn the_note_gives_the_code_while_gh_waits_and_the_reason_after_a_failure() {
        let offered = SignIn::offered(SignInOffer::GhRefresh {
            login: "octocat".into(),
        });
        assert!(!offered.in_progress());
        assert!(!offered.in_app());
        assert_eq!(offered.note(), None);
        let waiting = SignIn::Waiting {
            login: "octocat".into(),
            code: "AB12-CD34".into(),
        };
        assert!(waiting.in_progress());
        assert!(
            waiting
                .note()
                .is_some_and(|note| note.contains("AB12-CD34") && note.contains("login/device"))
        );
        let failed = SignIn::Offered {
            offer: SignInOffer::GhRefresh {
                login: "octocat".into(),
            },
            failure: Some("GitHub sign-in did not finish: gh failed.".into()),
        };
        assert!(!failed.in_progress(), "a failed sign-in can be tried again");
        assert_eq!(
            failed.note().as_deref(),
            Some("GitHub sign-in did not finish: gh failed.")
        );
    }

    #[test]
    fn a_sign_in_in_subspace_has_no_gh_note() {
        let offered = SignIn::offered(SignInOffer::InApp { login: None });
        assert!(offered.in_app());
        assert!(!offered.in_progress());
        assert_eq!(offered.note(), None);
    }

    mod panel {
        use std::sync::Arc;

        use gpui_kit::TestAppContext;
        use gpui_kit::test::TestWindowExt as _;
        use subspace_agent::{Command, Event, SignInOffer};
        use subspace_core::Secret;
        use subspace_dashboard::github_sign_in::{self, Phase, SignInEvent};
        use subspace_github_auth::{AuthError, Authenticator, Started};

        use super::super::ChatPanel;
        use crate::prefs;

        struct Unused;

        impl Authenticator for Unused {
            fn start(&self) -> Result<Started, AuthError> {
                Err(AuthError::Cancelled)
            }

            fn revoke(&self, _: &Secret) {}
        }

        #[gpui_kit::test]
        fn a_missing_account_offers_the_subspace_sign_in_and_reconnects_after_it(
            cx: &mut TestAppContext,
        ) {
            let directory = tempfile::tempdir().unwrap();
            let (handle, mut commands) = subspace_agent::AgentHandle::test_offline(
                directory.path().into(),
                vec![
                    Event::SignInRequired(SignInOffer::InApp { login: None }),
                    Event::Error("No GitHub account found.".into()),
                ],
            );
            let shared = cx.update(|cx| {
                cx.set_reduce_motion(true);
                gpui_kit::init(cx);
                prefs::init_file(None, cx);
                github_sign_in::install(Arc::new(Unused), cx)
            });
            let (chat, cx) = cx.add_window_view(|window, cx| {
                ChatPanel::with_engine(subspace_core::config::ChatEngine::Copilot, window, cx)
            });
            cx.update(|_, cx| {
                chat.update(cx, |chat, cx| {
                    chat.connected(Ok(handle));
                    chat.poll(cx);
                    assert!(
                        chat.sign_in
                            .as_ref()
                            .is_some_and(super::super::SignIn::in_app)
                    );
                })
            });
            cx.update(|_, cx| shared.update(cx, |sign_in, cx| sign_in.show(Phase::Browser, cx)));
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                assert!(window.find("chat-sign-in-cancel").visible());
                assert!(chat.read(cx).signing_in(cx));
            });
            cx.update(|_, cx| {
                shared.update(cx, |_, cx| {
                    cx.emit(SignInEvent::SignedIn("octo-dev".into()))
                });
            });
            cx.run_until_parked();
            assert!(matches!(commands.try_recv(), Ok(Command::Shutdown)));
        }
    }
}
