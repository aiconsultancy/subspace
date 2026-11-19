//! Self-generated RSA fixture, never an application credential.
use super::{EntraClaims, OidcClient, Reply, identity};
use crate::Error;
use chrono::{Duration, Utc};
use openidconnect::core::{
    CoreGenderClaim, CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm,
    CoreProviderMetadata, CoreRsaPrivateSigningKey,
};
use openidconnect::{
    AccessToken, Audience, ClientId, EndUserUsername, IdToken, IdTokenClaims, IssuerUrl,
    JsonWebKeySet, Nonce, PrivateSigningKey, StandardClaims, SubjectIdentifier,
};
const ISSUER: &str = "https://login.microsoftonline.com/00000000-0000-0000-0000-000000000001/v2.0";
fn key() -> &'static CoreRsaPrivateSigningKey {
    use rsa::pkcs1::EncodeRsaPrivateKey as _;
    static KEY: std::sync::OnceLock<CoreRsaPrivateSigningKey> = std::sync::OnceLock::new();
    KEY.get_or_init(|| {
        let generated = rsa::RsaPrivateKey::new(&mut rand_fixture::thread_rng(), 2048).unwrap();
        let pem = generated.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF).unwrap();
        CoreRsaPrivateSigningKey::from_pem(&pem, None).unwrap()
    })
}
fn client() -> OidcClient {
    let metadata: CoreProviderMetadata = serde_json::from_value(serde_json::json!({
        "issuer":ISSUER,"authorization_endpoint":"https://login.microsoftonline.com/authorize",
        "token_endpoint":"https://login.microsoftonline.com/token","jwks_uri":"https://login.microsoftonline.com/keys",
        "response_types_supported":["code"],"subject_types_supported":["pairwise"],"id_token_signing_alg_values_supported":["RS256"]
    })).unwrap();
    OidcClient::from_provider_metadata(
        metadata.set_jwks(JsonWebKeySet::new(vec![key().as_verification_key()])),
        ClientId::new("desktop-fixture".into()),
        None,
    )
}
fn reply(audience: &str, issuer: &str, seconds: i64, nonce: Option<&str>) -> Reply {
    let claims = IdTokenClaims::new(
        IssuerUrl::new(issuer.into()).unwrap(),
        vec![Audience::new(audience.into())],
        Utc::now() + Duration::seconds(seconds),
        Utc::now() - Duration::seconds(10),
        StandardClaims::<CoreGenderClaim>::new(SubjectIdentifier::new("fixture-subject".into()))
            .set_preferred_username(Some(EndUserUsername::new("developer@example.test".into()))),
        EntraClaims {
            tid: "00000000-0000-0000-0000-000000000001".into(),
            oid: "00000000-0000-0000-0000-000000000003".into(),
        },
    )
    .set_nonce(nonce.map(|n| Nonce::new(n.into())));
    let token: IdToken<
        EntraClaims,
        CoreGenderClaim,
        CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm,
    > = IdToken::new(
        claims,
        key(),
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
        Some(&AccessToken::new("fixture-access".into())),
        None,
    )
    .unwrap();
    serde_json::from_value(serde_json::json!({"access_token":"fixture-access","token_type":"Bearer","expires_in":3600,"id_token":token})).unwrap()
}
#[test]
fn signed_identity_requires_the_expected_issuer_audience_expiry_and_nonce() {
    let client = client();
    let nonce = Nonce::new("expected".into());
    let valid = reply("desktop-fixture", ISSUER, 3600, Some("expected"));
    assert_eq!(
        identity(&client, &valid, &nonce, false).unwrap().0.username,
        "developer@example.test"
    );
    for invalid in [
        reply("other-client", ISSUER, 3600, Some("expected")),
        reply(
            "desktop-fixture",
            "https://attacker.example.test",
            3600,
            Some("expected"),
        ),
        reply("desktop-fixture", ISSUER, -3600, Some("expected")),
        reply("desktop-fixture", ISSUER, 3600, Some("wrong")),
        reply("desktop-fixture", ISSUER, 3600, None),
    ] {
        assert_eq!(
            identity(&client, &invalid, &nonce, false),
            Err(Error::InvalidReply)
        );
    }
    assert!(
        identity(
            &client,
            &reply("desktop-fixture", ISSUER, 3600, None),
            &nonce,
            true
        )
        .is_ok()
    );
}

#[test]
fn a_signed_identity_rejects_tampered_signatures_and_substituted_access_tokens() {
    let client = client();
    let nonce = Nonce::new("expected".into());
    let valid = reply("desktop-fixture", ISSUER, 3600, Some("expected"));
    let mut tampered = serde_json::to_value(&valid).unwrap();
    let mut jwt = tampered["id_token"].as_str().unwrap().to_owned();
    let signature = jwt.rfind('.').unwrap() + 1;
    let replacement = if &jwt[signature..signature + 1] == "A" {
        "B"
    } else {
        "A"
    };
    jwt.replace_range(signature..signature + 1, replacement);
    tampered["id_token"] = jwt.into();
    assert_eq!(
        identity(
            &client,
            &serde_json::from_value(tampered).unwrap(),
            &nonce,
            false
        ),
        Err(Error::InvalidReply)
    );
    let mut substituted = serde_json::to_value(valid).unwrap();
    substituted["access_token"] = "substituted".into();
    assert_eq!(
        identity(
            &client,
            &serde_json::from_value(substituted).unwrap(),
            &nonce,
            false
        ),
        Err(Error::InvalidReply)
    );
}
