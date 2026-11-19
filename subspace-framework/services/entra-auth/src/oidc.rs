use crate::{Account, Config, Error, http};
use openidconnect::core::{
    CoreAuthDisplay, CoreAuthPrompt, CoreAuthenticationFlow, CoreErrorResponseType,
    CoreGenderClaim, CoreJsonWebKey, CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm,
    CoreProviderMetadata, CoreRevocableToken, CoreRevocationErrorResponse,
    CoreTokenIntrospectionResponse, CoreTokenType,
};
use openidconnect::{
    AccessTokenHash, AdditionalClaims, AuthorizationCode, Client, ClientId, CsrfToken,
    EmptyExtraTokenFields, EndpointMaybeSet, EndpointNotSet, EndpointSet, IdTokenFields, IssuerUrl,
    Nonce, NonceVerifier, OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl,
    RefreshToken, RequestTokenError, Scope, StandardErrorResponse, StandardTokenResponse,
    TokenResponse,
};
use serde::{Deserialize, Serialize};
use subspace_core::Secret;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct EntraClaims {
    tid: String,
    oid: String,
}
impl AdditionalClaims for EntraClaims {}

type Reply = StandardTokenResponse<
    IdTokenFields<
        EntraClaims,
        EmptyExtraTokenFields,
        CoreGenderClaim,
        CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm,
    >,
    CoreTokenType,
>;
type OidcClient = Client<
    EntraClaims,
    CoreAuthDisplay,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJsonWebKey,
    CoreAuthPrompt,
    StandardErrorResponse<CoreErrorResponseType>,
    Reply,
    CoreTokenIntrospectionResponse,
    CoreRevocableToken,
    CoreRevocationErrorResponse,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

#[derive(Clone)]
pub(crate) struct Session {
    pub account: Account,
    pub subject: String,
    pub refresh: Secret,
    pub nonce: Secret,
}

pub(crate) struct Granted {
    pub session: Session,
    pub access: Secret,
    pub expires_at: std::time::Instant,
}

pub(crate) trait Provider: Send + Sync {
    fn refresh(&self, session: &Session, scope: Option<&str>) -> Result<Granted, Error>;
}

pub(crate) struct Entra(pub Config);

pub(crate) struct Exchange {
    client: OidcClient,
    verifier: PkceCodeVerifier,
    nonce: Nonce,
    config: Config,
}

pub(crate) fn prepare(
    config: &Config,
    redirect: &str,
    scope: Option<&str>,
) -> Result<(String, String, Exchange), Error> {
    let client = client(config)?
        .set_redirect_uri(RedirectUrl::new(redirect.to_owned()).map_err(|_| Error::Listener)?);
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let request = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("profile".into()))
        .add_scope(Scope::new("offline_access".into()))
        .add_prompt(CoreAuthPrompt::SelectAccount)
        .set_pkce_challenge(challenge)
        .add_scopes(scope.map(|scope| Scope::new(scope.into())));
    let (url, state, nonce) = request.url();
    Ok((
        url.into(),
        state.secret().clone(),
        Exchange {
            client,
            verifier,
            nonce,
            config: config.clone(),
        },
    ))
}

fn client(config: &Config) -> Result<OidcClient, Error> {
    let issuer = IssuerUrl::new(config.issuer()).map_err(|_| Error::Unconfigured)?;
    let metadata =
        CoreProviderMetadata::discover(&issuer, &http::send).map_err(|_| Error::Offline)?;
    // Discovery may only direct identity traffic to the configured authority.
    for endpoint in [
        Some(metadata.authorization_endpoint().url()),
        metadata.token_endpoint().map(|v| v.url()),
        Some(metadata.jwks_uri().url()),
    ]
    .into_iter()
    .flatten()
    {
        if endpoint.scheme() != "https"
            || endpoint.host_str() != Some(config.authority_host())
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
        {
            return Err(Error::InvalidReply);
        }
    }
    Ok(OidcClient::from_provider_metadata(
        metadata,
        ClientId::new(config.client.clone()),
        None,
    ))
}

impl Exchange {
    pub fn redeem(self, code: String) -> Result<Granted, Error> {
        let reply = self
            .client
            .exchange_code(AuthorizationCode::new(code))
            .map_err(|_| Error::Service)?
            .set_pkce_verifier(self.verifier)
            .request(&http::send)
            .map_err(token_error)?;
        let identity = identity(&self.client, &reply, &self.nonce, false)?;
        if identity.0.tenant != self.config.tenant {
            return Err(Error::InvalidReply);
        }
        let refresh = reply.refresh_token().ok_or(Error::SignInRequired)?;
        let session = Session {
            account: identity.0,
            subject: identity.1,
            refresh: Secret::new(refresh.secret()),
            nonce: Secret::new(self.nonce.secret()),
        };
        granted(session, &reply)
    }
}

impl Provider for Entra {
    fn refresh(&self, session: &Session, scope: Option<&str>) -> Result<Granted, Error> {
        let client = client(&self.0)?;
        let refresh = RefreshToken::new(session.refresh.expose().to_owned());
        let request = client
            .exchange_refresh_token(&refresh)
            .map_err(|_| Error::Service)?
            .add_scope(Scope::new("openid".into()))
            .add_scope(Scope::new("profile".into()))
            .add_scope(Scope::new("offline_access".into()))
            .add_scopes(scope.map(|scope| Scope::new(scope.into())));
        let reply = request.request(&http::send).map_err(token_error)?;
        granted(refresh_session(&client, &reply, session)?, &reply)
    }
}

fn refresh_session(
    client: &OidcClient,
    reply: &Reply,
    session: &Session,
) -> Result<Session, Error> {
    let nonce = Nonce::new(session.nonce.expose().to_owned());
    let account = reply
        .id_token()
        .map(|_| identity(client, reply, &nonce, true))
        .transpose()?
        .map(|identity| refreshed_account(session, identity))
        .transpose()?
        .unwrap_or_else(|| session.account.clone());
    Ok(Session {
        account,
        subject: session.subject.clone(),
        nonce: session.nonce.clone(),
        refresh: reply
            .refresh_token()
            .map(|refresh| Secret::new(refresh.secret()))
            .unwrap_or_else(|| session.refresh.clone()),
    })
}

fn refreshed_account(session: &Session, identity: (Account, String)) -> Result<Account, Error> {
    if identity.0.tenant != session.account.tenant
        || identity.0.id != session.account.id
        || identity.1 != session.subject
    {
        Err(Error::InvalidReply)
    } else {
        Ok(identity.0)
    }
}

fn identity(
    client: &OidcClient,
    reply: &Reply,
    nonce: &Nonce,
    refreshing: bool,
) -> Result<(Account, String), Error> {
    let token = reply.id_token().ok_or(Error::InvalidReply)?;
    let verifier = client.id_token_verifier();
    let claims = token
        .claims(&verifier, |actual: Option<&Nonce>| {
            if refreshing && actual.is_none() {
                Ok(())
            } else {
                nonce.verify(actual)
            }
        })
        .map_err(|_| Error::InvalidReply)?;
    if let Some(expected) = claims.access_token_hash() {
        let actual = AccessTokenHash::from_token(
            reply.access_token(),
            token.signing_alg().map_err(|_| Error::InvalidReply)?,
            token
                .signing_key(&verifier)
                .map_err(|_| Error::InvalidReply)?,
        )
        .map_err(|_| Error::InvalidReply)?;
        if &actual != expected {
            return Err(Error::InvalidReply);
        }
    }
    let extra = claims.additional_claims();
    let tenant = uuid::Uuid::parse_str(&extra.tid)
        .map_err(|_| Error::InvalidReply)?
        .to_string();
    let id = uuid::Uuid::parse_str(&extra.oid)
        .map_err(|_| Error::InvalidReply)?
        .to_string();
    let username = claims
        .preferred_username()
        .map(|value| value.as_str().to_owned())
        .unwrap_or_else(|| "Microsoft account".into());
    let name = claims
        .name()
        .and_then(|names| names.get(None))
        .map(|v| v.to_string())
        .unwrap_or_else(|| username.clone());
    Ok((
        Account {
            tenant,
            id,
            name,
            username,
        },
        claims.subject().as_str().to_owned(),
    ))
}

fn granted(session: Session, reply: &Reply) -> Result<Granted, Error> {
    let lifetime = reply
        .expires_in()
        .filter(|v| v > &crate::session::EXPIRY_SKEW)
        .ok_or(Error::InvalidReply)?;
    Ok(Granted {
        session,
        access: Secret::new(reply.access_token().secret()),
        expires_at: std::time::Instant::now()
            .checked_add(lifetime.saturating_sub(crate::session::EXPIRY_SKEW))
            .ok_or(Error::InvalidReply)?,
    })
}

fn token_error(
    error: RequestTokenError<http::TransportError, StandardErrorResponse<CoreErrorResponseType>>,
) -> Error {
    match error {
        RequestTokenError::Request(_) => Error::Offline,
        RequestTokenError::ServerResponse(response) => match response.error() {
            CoreErrorResponseType::InvalidGrant => Error::SignInRequired,
            CoreErrorResponseType::UnauthorizedClient | CoreErrorResponseType::InvalidClient => {
                Error::NotPermitted
            }
            CoreErrorResponseType::Extension(code)
                if matches!(
                    code.as_str(),
                    "interaction_required" | "login_required" | "consent_required"
                ) =>
            {
                Error::SignInRequired
            }
            CoreErrorResponseType::Extension(code) if code == "access_denied" => {
                Error::NotPermitted
            }
            _ => Error::Service,
        },
        _ => Error::InvalidReply,
    }
}

#[cfg(test)]
mod tests;
