//! The browser leg: a loopback callback on `localhost`, whose port Entra
//! ignores, and the code exchange once the browser returns.

use crate::oidc::{self, Exchange, Granted};
use crate::{Account, Cancel, Config, Error};
use subspace_account::loopback::Loopback;

const CALLBACK: &str = "/auth/callback";

pub(crate) struct Pending {
    loopback: Loopback,
    exchange: Exchange,
    state: String,
    pub(crate) resource: Option<String>,
}

pub(crate) fn start(config: &Config, resource: Option<&str>) -> Result<(String, Pending), Error> {
    let scope = resource.map(|name| config.scope(name)).transpose()?;
    // The registration uses localhost, whose port Entra ignores. Bind IPv4
    // loopback only; never a wildcard interface or a fixed shared port.
    let loopback = Loopback::bind("localhost", 0, CALLBACK)?;
    let (url, state, exchange) = oidc::prepare(config, &loopback.redirect_uri(), scope)?;
    Ok((
        url,
        Pending {
            loopback,
            exchange,
            state,
            resource: resource.map(str::to_owned),
        },
    ))
}

impl Pending {
    pub fn finish(
        self,
        cancel: &Cancel,
        accept: impl FnOnce(Granted) -> Result<Account, Error>,
    ) -> Result<Account, Error> {
        let reply = self.loopback.wait(cancel, &self.state)?;
        let exchange = self.exchange;
        reply.complete(cancel, |code| exchange.redeem(code).and_then(accept))
    }
}
