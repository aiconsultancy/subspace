//! Starts and completes the browser and device authorisation flows.

use super::{AUTHENTICATE, Access, Session, SessionManager, device, reply::granted};
use crate::config::Flow;
use subspace_account::loopback::{Loopback, Pkce, random_value};
use subspace_account::{Account, Cancel, Error, Started};

const CALLBACK: &str = "/callback";
const AUTHORIZE: &str = "/user_management/authorize";
const AUTHORIZE_DEVICE: &str = "/user_management/authorize/device";

pub(super) enum Pending {
    Browser {
        loopback: Loopback,
        pkce: Pkce,
        state: String,
    },
    Device(device::Device),
}

impl SessionManager {
    pub(super) fn start_sign_in(&self) -> Result<Started, Error> {
        if self.config.flow == Flow::Device {
            return self.start_device();
        }
        let loopback = Loopback::bind("127.0.0.1", self.config.redirect_port, CALLBACK)?;
        let pkce = Pkce::new();
        let state = random_value();
        let url = self.authorize_url(&loopback.redirect_uri(), &pkce, &state)?;
        self.pending.start(
            Some(url),
            Pending::Browser {
                loopback,
                pkce,
                state,
            },
        )
    }

    pub(super) fn finish_sign_in(
        &self,
        started: Started,
        cancel: &Cancel,
        generation: u64,
    ) -> Result<Account, Error> {
        match self.pending.take(&started)? {
            Pending::Browser {
                loopback,
                pkce,
                state,
            } => {
                let reply = loopback.wait(cancel, &state)?;
                reply.complete(cancel, |code| {
                    let granted = self.redeem(&code, &pkce)?;
                    self.accept(granted, cancel, generation)
                })
            }
            Pending::Device(device) => {
                let reply = device::poll(
                    self.transport.as_ref(),
                    self.clock.as_ref(),
                    device::PollRequest {
                        url: &self.config.endpoint(AUTHENTICATE),
                        client_id: &self.config.client_id,
                    },
                    &device,
                    cancel,
                )?;
                self.accept(granted(reply, None)?, cancel, generation)
            }
        }
    }

    fn authorize_url(&self, redirect: &str, pkce: &Pkce, state: &str) -> Result<String, Error> {
        let mut url =
            url::Url::parse(&self.config.endpoint(AUTHORIZE)).map_err(|_| Error::Unconfigured)?;
        url.query_pairs_mut()
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", redirect)
            .append_pair("response_type", "code")
            .append_pair("provider", "authkit")
            .append_pair("state", state)
            .append_pair("code_challenge", &pkce.challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url.into())
    }

    pub(super) fn redeem(&self, code: &str, pkce: &Pkce) -> Result<(Session, Access), Error> {
        let reply = self.transport.authenticate(
            &self.config.endpoint(AUTHENTICATE),
            serde_json::json!({
                "client_id": self.config.client_id,
                "grant_type": "authorization_code",
                "code": code,
                "code_verifier": pkce.verifier.expose(),
            }),
        )?;
        granted(reply, None)
    }

    /// Ask WorkOS for a device code; the shell opens the verification page
    /// and shows the user code.
    fn start_device(&self) -> Result<Started, Error> {
        let issued = self.clock.now();
        let start = self.transport.authorize_device(
            &self.config.endpoint(AUTHORIZE_DEVICE),
            &self.config.client_id,
        )?;
        let (prompt, device) = device::validate(start, issued, self.config.loopback())?;
        self.pending.start_with_code(
            Some(prompt.url),
            Some(prompt.user_code),
            Pending::Device(device),
        )
    }
}
