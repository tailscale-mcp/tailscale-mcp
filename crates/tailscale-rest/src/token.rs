//! Turning a credential into the bearer token a request carries.
//!
//! An API access token is already a bearer token and goes on the wire as it
//! stands. The
//! other two credentials are exchanged at the token endpoint for a short-lived
//! one, which is then cached: an exchange per request would triple the traffic
//! and hand the control plane a rate limit to enforce on us.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::sync::Mutex;

use crate::credentials::Credentials;
use crate::error::ApiError;
use crate::secret::Secret;

/// Where the exchange posts. The vendored schema does not describe the OAuth
/// endpoints (ADR-0002), so this path is written from Tailscale's own
/// documentation rather than generated.
pub(crate) const TOKEN_PATH: &str = "/api/v2/oauth/token";

/// RFC 7523's name for "the client is authenticating with a JWT", which is how
/// a federated identity proves itself without holding a client secret.
const JWT_ASSERTION: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

/// How long before its stated expiry a token is treated as spent.
///
/// A token that expires while in flight fails a call the caller would have to
/// make again, so the last minute of every token's life is given up.
pub(crate) const REFRESH_SKEW: Duration = Duration::from_secs(60);

/// What a token is assumed to be worth when the server does not say.
///
/// Tailscale always sends `expires_in`. Assuming a long life if it ever stops
/// would turn one missing field into an hour of 401s, so the assumption is
/// short enough to cost a few extra exchanges and nothing else.
const ASSUMED_LIFETIME: Duration = Duration::from_secs(300);

/// The token a request carries, and which minting it came from.
#[derive(Debug)]
pub(crate) struct Bearer {
    pub value: Secret,
    /// `None` for a credential that was never minted and cannot be re-minted.
    /// A rejected access token is a bad access token; there is nothing to evict.
    pub generation: Option<u64>,
}

/// What was minted, and until when.
#[derive(Debug)]
struct Minted {
    value: Secret,
    expires_at: Instant,
    generation: u64,
}

/// The token endpoint's answer.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    /// Seconds. Absent on a server that does not send it, which Tailscale's
    /// does; see [`ASSUMED_LIFETIME`].
    #[serde(default)]
    expires_in: Option<u64>,
}

/// A credential, and the token it was last exchanged for.
#[derive(Debug)]
pub(crate) struct Tokens {
    credentials: Credentials,
    endpoint: String,
    http: reqwest::Client,
    /// One mutex around the whole exchange rather than a lock-free cache: two
    /// requests arriving on a cold cache should produce one exchange, and the
    /// only thing the second can usefully do meanwhile is wait for the first.
    /// Nothing can proceed without a token anyway.
    minted: Mutex<Option<Minted>>,
    generations: AtomicU64,
}

impl Tokens {
    pub(crate) fn new(credentials: Credentials, base_url: &str, http: reqwest::Client) -> Self {
        Self {
            credentials,
            endpoint: format!("{base_url}{TOKEN_PATH}"),
            http,
            minted: Mutex::new(None),
            generations: AtomicU64::new(0),
        }
    }

    /// Whether a refused token can be replaced by minting another.
    ///
    /// A refused access token is a bad access token, and minting is not a thing
    /// that can be done with one, so a call carrying one gives up where a call
    /// carrying a minted token tries once more.
    pub(crate) const fn can_refresh(&self) -> bool {
        !matches!(self.credentials, Credentials::ApiKey(_))
    }

    /// The token to send, minting one if what is cached is spent.
    pub(crate) async fn bearer(&self) -> Result<Bearer, ApiError> {
        let Credentials::ApiKey(key) = &self.credentials else {
            return self.minted_bearer().await;
        };
        Ok(Bearer {
            value: key.clone(),
            generation: None,
        })
    }

    async fn minted_bearer(&self) -> Result<Bearer, ApiError> {
        let mut held = self.minted.lock().await;
        if let Some(current) = held.as_ref()
            && Instant::now() + REFRESH_SKEW < current.expires_at
        {
            return Ok(Bearer {
                value: current.value.clone(),
                generation: Some(current.generation),
            });
        }

        let fresh = self.exchange().await?;
        let generation = self.generations.fetch_add(1, Ordering::Relaxed);
        let bearer = Bearer {
            value: fresh.0.clone(),
            generation: Some(generation),
        };
        *held = Some(Minted {
            value: fresh.0,
            expires_at: Instant::now() + fresh.1,
            generation,
        });
        Ok(bearer)
    }

    /// Throw away a token the control plane refused.
    ///
    /// The generation is what makes this happen once. Several requests in
    /// flight with the same token all get the same 401 back; the first eviction
    /// wins and the rest find a generation that has moved on and leave the
    /// freshly minted token alone.
    pub(crate) async fn evict(&self, generation: u64) {
        let mut held = self.minted.lock().await;
        if held.as_ref().is_some_and(|m| m.generation == generation) {
            *held = None;
        }
    }

    /// One exchange at the token endpoint. The caller's retry loop decides
    /// whether a transient failure here is worth another go.
    async fn exchange(&self) -> Result<(Secret, Duration), ApiError> {
        let mut form: Vec<(&str, String)> = vec![("grant_type", "client_credentials".to_owned())];
        match &self.credentials {
            // Handled by the caller; a key is its own bearer.
            Credentials::ApiKey(_) => {
                return Err(ApiError::Token(
                    "an API access token is used directly and is never exchanged".to_owned(),
                ));
            }
            Credentials::OauthClient {
                client_id,
                client_secret,
                scopes,
            } => {
                form.push(("client_id", client_id.clone()));
                form.push(("client_secret", client_secret.expose().to_owned()));
                push_scopes(&mut form, scopes);
            }
            Credentials::Federated {
                client_id,
                jwt_file,
                scopes,
            } => {
                // Read every time: the platform rotates this file underneath
                // us, and a cached assertion outlives its own signature.
                let jwt =
                    std::fs::read_to_string(jwt_file).map_err(|source| ApiError::JwtFile {
                        path: jwt_file.clone(),
                        source,
                    })?;
                if let Some(client_id) = client_id {
                    form.push(("client_id", client_id.clone()));
                }
                form.push(("client_assertion_type", JWT_ASSERTION.to_owned()));
                form.push(("client_assertion", jwt.trim().to_owned()));
                push_scopes(&mut form, scopes);
            }
        }

        tracing::debug!(
            endpoint = %self.endpoint,
            kind = self.credentials.kind(),
            "exchanging the control-plane credential for a token"
        );

        let request = format!("POST {TOKEN_PATH}");
        let response = self
            .http
            .post(&self.endpoint)
            .form(&form)
            .send()
            .await
            .map_err(|source| ApiError::Transport {
                request: request.clone(),
                source,
            })?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(ApiError::Status {
                request,
                status: status.as_u16(),
                // The body of a failed exchange describes the client, not the
                // secret: `invalid_client`, `invalid_scope`. Sending it on is
                // the difference between a fixable error and a shrug.
                message: crate::error::describe(status, &body),
                retry_after: None,
            });
        }

        let parsed: TokenResponse = serde_json::from_str(&body).map_err(|source| {
            // Deliberately not `Malformed`: a body that is not a token is not
            // a result the caller asked for, and naming the endpoint is more
            // use than naming serde's position in a body nobody will see.
            ApiError::Token(format!("the token endpoint answered with {source}"))
        })?;
        if parsed.access_token.trim().is_empty() {
            return Err(ApiError::Token(
                "the token endpoint answered with an empty access token".to_owned(),
            ));
        }
        let lifetime = parsed
            .expires_in
            .map_or(ASSUMED_LIFETIME, Duration::from_secs);
        Ok((Secret::new(parsed.access_token), lifetime))
    }
}

fn push_scopes(form: &mut Vec<(&'static str, String)>, scopes: &[String]) {
    if !scopes.is_empty() {
        // OAuth 2 spells a scope list space-separated, whatever separator the
        // environment variable was written with.
        form.push(("scope", scopes.join(" ")));
    }
}
