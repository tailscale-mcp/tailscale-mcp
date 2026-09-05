//! A session removes the credential it was given, not only key-shaped text.
//!
//! `Redactor` has two passes. The shape pass removes anything that looks like
//! a Tailscale key — `tskey-…`, `privkey:`, `nlpriv:`, `Bearer …` — and needs
//! to know nothing about the session. The literal pass removes the specific
//! values this session holds, and exists, in its own words, because the OAuth
//! client secret "need not look like a Tailscale key".
//!
//! The literal pass did nothing for four releases. `Redactor::default()` is an
//! empty list, `server::build` constructed one, and `with_secret` was reachable
//! from the unit tests and from nowhere else — so the type documented itself as
//! "built once at startup from whatever credentials were configured" while
//! being built from none of them.
//!
//! The exposure was narrow, because the shape rules cover every credential
//! Tailscale currently issues and `Secret` refuses to print itself. It was the
//! guarantee that was wrong rather than the outcome, which is the kind that
//! gets relied on later: the next diagnostic that puts a client secret in front
//! of a model would have been covered by a comment and nothing else.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use clap::Parser as _;
use tailscale_mcp::config::{Cli, Config};
use tailscale_mcp::error::Redactor;
use tailscale_mcp::server::{self, Backends};
use tailscale_rest::{Credentials, Secret};

/// Deliberately not key-shaped: the shape pass must not be what removes it, or
/// this proves nothing about the literal pass.
const UNSHAPED: &str = "an-oauth-client-secret-that-looks-like-nothing";

fn oauth(secret: &str) -> Credentials {
    Credentials::OauthClient {
        client_id: "kExampleCNTRL".to_owned(),
        client_secret: Secret::new(secret),
        scopes: vec!["all".to_owned()],
    }
}

#[test]
fn a_session_removes_the_oauth_secret_it_was_configured_with() {
    let redactor = Redactor::for_credentials(Some(&oauth(UNSHAPED)));
    let said = format!("the client rejected `{UNSHAPED}` at the token endpoint");
    let scrubbed = redactor.apply(&said);
    assert!(
        !scrubbed.contains(UNSHAPED),
        "the configured secret should not survive: {scrubbed}"
    );
}

#[test]
fn a_session_removes_the_api_key_it_was_configured_with() {
    // Key-shaped, so the shape pass would catch it too — which is the point:
    // both passes have to agree, and a session must never be the only thing
    // standing between its own credential and a message.
    let key = "tskey-api-nExAmPlE1-redactedKeyValue";
    let redactor = Redactor::for_credentials(Some(&Credentials::ApiKey(Secret::new(key))));
    let said = format!("GET failed with `{key}`");
    let scrubbed = redactor.apply(&said);
    assert!(!scrubbed.contains(key), "{scrubbed}");
}

/// The bug this file is named for: a redactor built from nothing.
#[test]
fn the_literal_pass_is_empty_without_a_credential_and_that_is_the_bug_it_had() {
    let bare = Redactor::default();
    assert!(
        bare.apply(UNSHAPED).contains(UNSHAPED),
        "a redactor built from no credentials cannot remove one — which is why \
         `for_credentials` exists and why `server::build` must call it"
    );

    // And a session with no credential at all is that same redactor, correctly:
    // there is nothing to register, and the shape rules still apply.
    let none = Redactor::for_credentials(None);
    assert!(none.apply(UNSHAPED).contains(UNSHAPED));
    assert!(
        !none
            .apply("tskey-api-nExAmPlE1-value")
            .contains("nExAmPlE1"),
        "the shape rules do not depend on a credential"
    );
}

/// A federated credential registers nothing, and that is deliberate.
#[test]
fn a_federated_credential_has_no_value_to_register_at_startup() {
    let federated = Credentials::Federated {
        client_id: Some("kExampleCNTRL".to_owned()),
        jwt_file: std::path::PathBuf::from("/example/token"),
        scopes: vec!["all".to_owned()],
    };
    let redactor = Redactor::for_credentials(Some(&federated));
    // The JWT is read at exchange time and never held, so there is nothing to
    // scrub by value. What it is exchanged for is shaped, and is covered.
    assert!(
        !redactor
            .apply("Authorization: Bearer eyJhbGciOiJFUzI1NiJ9.example")
            .contains("eyJhbGciOiJFUzI1NiJ9.example"),
        "the token a JWT is exchanged for is bearer-shaped and is removed"
    );
}

/// And the server actually asks for one.
///
/// The three tests above exercise `for_credentials` directly, and would all
/// still pass with `server::build` constructing a `Redactor::default()` — which
/// is exactly the state this file was written about. This one goes through the
/// real build and reads the redactor the session will use.
#[tokio::test]
async fn the_server_builds_its_redactor_from_the_session_credential() {
    let cli = Cli::try_parse_from(["tailscale-mcp", "--no-local"]).expect("the arguments parse");
    let config = Config::resolve_with(cli, |_| None).expect("the configuration resolves");
    let backends = Backends {
        local: std::sync::Arc::new(tailscale_cli::stub::StubBackend::missing()),
        local_available: false,
        credentials: Some(oauth(UNSHAPED)),
    };
    let startup = server::build(&config, tailscale_mcp::tools::entries(), backends)
        .await
        .expect("the server builds");

    let said = format!("the token endpoint refused `{UNSHAPED}`");
    let scrubbed = startup.server.context().redactor.apply(&said);
    assert!(
        !scrubbed.contains(UNSHAPED),
        "the session's own redactor still holds no secrets: {scrubbed}"
    );
}
