//! The transport: one client, and the rules every call goes through.
//!
//! Everything above this module builds a path and reads a body. What lives
//! here is the part that is the same for all 93 tailnet tools — which
//! credential goes on the wire, when a failed call is worth repeating, how many
//! may be in flight at once, and how large an answer this server will hold.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::Semaphore;

use crate::credentials::{Credentials, DEFAULT_TAILNET};
use crate::error::{ApiError, Idempotence, describe};
use crate::token::Tokens;

/// The control plane. Pinned rather than configured, because a server that can
/// be pointed at another host is a server that can be pointed at an attacker's.
pub const DEFAULT_BASE_URL: &str = "https://api.tailscale.com";

/// How long one call may take, across every attempt it makes.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(30);

/// How many calls may be in flight at once.
///
/// The control plane rate-limits per tailnet, and an agent that fans out over
/// a device list can produce a hundred calls from one thought. Holding the
/// fan-in here turns that into a queue rather than a wall of 429s.
pub const DEFAULT_CONCURRENCY: usize = 8;

/// The two statuses this module decides something about, named so that the
/// decision and the check are spelled the same way wherever they appear.
const UNAUTHORIZED: u16 = 401;
const TOO_MANY_REQUESTS: u16 = 429;

/// How many times one call is attempted, first try included.
const MAX_ATTEMPTS: u32 = 4;

/// The first backoff, doubled per retry.
const BASE_BACKOFF: Duration = Duration::from_millis(250);

/// The longest wait honoured from a `Retry-After`.
///
/// A server asking for ten minutes is asking for longer than any call budget,
/// and sleeping on it would only spend the budget doing nothing.
const MAX_BACKOFF: Duration = Duration::from_secs(20);

/// How much of a failed call's body is read before it is described.
///
/// The size cap is about results; an error message is not a result, and reading
/// a megabyte of one to print a sentence helps nobody.
const MAX_ERROR_BYTES: usize = 8 * 1024;

/// How to reach the control plane.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Where to send. [`DEFAULT_BASE_URL`] unless a test says otherwise.
    pub base_url: String,
    /// The tailnet a path means when the caller does not name one.
    pub tailnet: String,
    pub credentials: Credentials,
    /// The whole of one call, retries and backoff included.
    pub budget: Duration,
    pub concurrency: usize,
    /// The largest body this server will hold in memory.
    pub max_response_bytes: usize,
    pub user_agent: String,
}

impl ClientConfig {
    /// A configuration pointed at the real control plane.
    pub fn new(credentials: Credentials) -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_owned(),
            tailnet: DEFAULT_TAILNET.to_owned(),
            credentials,
            budget: DEFAULT_BUDGET,
            concurrency: DEFAULT_CONCURRENCY,
            max_response_bytes: 1 << 20,
            user_agent: format!("tailscale-mcp/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

/// A client for the control plane.
///
/// Cloning is cheap and shares everything: the connection pool, the token, and
/// — the point of sharing it — the concurrency limit. Two clients would be two
/// limits, which is one limit too many.
#[derive(Debug, Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    http: reqwest::Client,
    tokens: Tokens,
    base_url: String,
    tailnet: String,
    budget: Duration,
    max_response_bytes: usize,
    in_flight: Semaphore,
}

impl Client {
    pub fn new(config: ClientConfig) -> Result<Self, ApiError> {
        let base_url = checked_base_url(&config.base_url)?;
        if config.concurrency == 0 {
            return Err(ApiError::Config(
                "at least one call has to be allowed in flight".to_owned(),
            ));
        }
        if config.max_response_bytes == 0 {
            return Err(ApiError::Config(
                "a response size cap of zero would reject every answer".to_owned(),
            ));
        }

        let http = reqwest::Client::builder()
            .user_agent(config.user_agent)
            // Not the call budget: that covers every attempt together and is
            // applied around the retry loop. This is the ceiling on one of
            // them, so a stalled connection cannot eat a whole budget alone.
            .timeout(config.budget)
            .build()
            .map_err(|source| {
                ApiError::Config(format!("the HTTP client could not be built: {source}"))
            })?;

        Ok(Self {
            inner: Arc::new(Inner {
                tokens: Tokens::new(config.credentials, &base_url, http.clone()),
                http,
                base_url,
                tailnet: config.tailnet,
                budget: config.budget,
                max_response_bytes: config.max_response_bytes,
                in_flight: Semaphore::new(config.concurrency),
            }),
        })
    }

    /// The tailnet a call means when the caller does not name one.
    pub fn tailnet(&self) -> &str {
        &self.inner.tailnet
    }

    /// The path of a tailnet-scoped resource, for `tailnet` or the default.
    ///
    /// Every tailnet path is built here so that a tailnet name carrying a
    /// slash cannot reach into a path it was not given.
    pub fn tailnet_path(&self, tailnet: Option<&str>, rest: &str) -> String {
        let tailnet = tailnet.map_or(self.tailnet(), str::trim);
        let tailnet = if tailnet.is_empty() {
            self.tailnet()
        } else {
            tailnet
        };
        format!("/api/v2/tailnet/{}{rest}", escape(tailnet))
    }

    pub fn get(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(reqwest::Method::GET, path)
    }

    pub fn post(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(reqwest::Method::POST, path)
    }

    pub fn put(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(reqwest::Method::PUT, path)
    }

    pub fn patch(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(reqwest::Method::PATCH, path)
    }

    pub fn delete(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(reqwest::Method::DELETE, path)
    }

    fn request(&self, method: reqwest::Method, path: impl Into<String>) -> RequestBuilder<'_> {
        RequestBuilder {
            client: self,
            method,
            path: path.into(),
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            budget: self.inner.budget,
            broken: None,
        }
    }
}

/// One call, before it is sent.
#[derive(Debug)]
pub struct RequestBuilder<'a> {
    client: &'a Client,
    method: reqwest::Method,
    path: String,
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    body: Option<Body>,
    budget: Duration,
    /// A failure that happened while the call was being built, kept until
    /// there is somewhere to return it from.
    broken: Option<ApiError>,
}

/// What a request carries, and how it is spelled on the wire.
///
/// Two shapes rather than one, because the policy endpoints take a document
/// this server did not author and must not reformat.
#[derive(Debug, Clone)]
enum Body {
    Json(Value),
    Text { content_type: String, text: String },
}

impl RequestBuilder<'_> {
    /// A query parameter. Repeating a name sends it twice, which is how the
    /// API spells a list.
    #[must_use]
    pub fn query(mut self, name: &str, value: impl std::fmt::Display) -> Self {
        self.query.push((name.to_owned(), value.to_string()));
        self
    }

    /// A query parameter, if there is one to send.
    #[must_use]
    pub fn maybe_query(self, name: &str, value: Option<impl std::fmt::Display>) -> Self {
        match value {
            Some(value) => self.query(name, value),
            None => self,
        }
    }

    /// A header. `If-Match` on the policy file is what this is for.
    #[must_use]
    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_owned(), value.into()));
        self
    }

    /// A JSON body.
    #[must_use]
    pub fn json(mut self, body: &impl Serialize) -> Self {
        match serde_json::to_value(body) {
            Ok(value) => self.body = Some(Body::Json(value)),
            Err(source) => {
                self.broken.get_or_insert(ApiError::Config(format!(
                    "the request body could not be built: {source}"
                )));
            }
        }
        self
    }

    /// A body that is text rather than JSON, under a content type of its own.
    ///
    /// The policy file is the reason: it is HuJSON — JSON with comments and
    /// trailing commas — and a caller who wrote one wants it sent as written,
    /// comments included. Sending it as a JSON string would send the document
    /// quoted and escaped, which is a different document.
    #[must_use]
    pub fn text(mut self, content_type: &str, body: impl Into<String>) -> Self {
        self.body = Some(Body::Text {
            content_type: content_type.to_owned(),
            text: body.into(),
        });
        self
    }

    /// The whole of this call, retries and backoff included: past it the call
    /// ends as a timeout whatever it was doing.
    ///
    /// The default is the client's, which is the tool timeout; a tool that
    /// waits on something slower passes its own. One attempt is separately
    /// capped at the client's budget, so raising this raises how long a call
    /// may spend across attempts rather than how long one may stall.
    #[must_use]
    pub fn budget(mut self, budget: Duration) -> Self {
        self.budget = budget;
        self
    }

    /// Send it, and read the answer as JSON.
    ///
    /// An empty body — which is what a successful `DELETE` sends — comes back
    /// as [`Value::Null`] rather than as a failure to parse nothing.
    pub async fn send(self) -> Result<Value, ApiError> {
        let request = self.describe_request();
        let answer = self.send_raw().await?;
        parse(&answer.bytes, &request)
    }

    /// Send it, and read the answer as a particular shape.
    pub async fn send_as<T: DeserializeOwned>(self) -> Result<T, ApiError> {
        Ok(self.send_answer().await?.value)
    }

    /// Send it, and read the answer both ways at once.
    ///
    /// ADR-0003 asks for "the parsed model together with the raw body and the
    /// headers that matter", and this is why: a tool forwards the body it was
    /// given, unrenamed and with every field the control plane sent, while the
    /// server reads the typed value to decide what to do next. Parsing twice
    /// would be two chances to disagree, so the model is deserialised from the
    /// [`Value`] rather than from the bytes a second time.
    ///
    /// An empty body reads as [`Value::Null`], the same as [`send`] gives it,
    /// and `T` has to be a type that can read null — [`Value`] or `()` or an
    /// [`Option`]. A model cannot: every one of them carries a flattened map
    /// of unknown fields, which makes it a map to serde, and serde will not
    /// read a map from null. That is the right way round, because the
    /// endpoints that answer with nothing are the deletions, and a deletion
    /// has no model to answer with.
    ///
    /// [`send`]: RequestBuilder::send
    pub async fn send_answer<T: DeserializeOwned>(self) -> Result<Answer<T>, ApiError> {
        let request = self.describe_request();
        let answer = self.send_raw().await?;
        let raw = parse(&answer.bytes, &request)?;
        let value =
            T::deserialize(&raw).map_err(|source| ApiError::Malformed { request, source })?;
        Ok(Answer {
            value,
            raw,
            etag: answer.etag,
        })
    }

    /// Send it, and read the answer as text.
    ///
    /// The policy file is HuJSON — comments and trailing commas — so it is not
    /// JSON to parse, and its `ETag` is what a later write has to quote.
    pub async fn send_text(self) -> Result<TextBody, ApiError> {
        let answer = self.send_raw().await?;
        Ok(TextBody {
            // The API sends UTF-8; anything else is a corrupted body, and
            // replacing the bad bytes says so more usefully than a parse
            // error about an offset nobody can see.
            text: String::from_utf8_lossy(&answer.bytes).into_owned(),
            etag: answer.etag,
        })
    }

    /// `GET /api/v2/tailnet/-/devices`, for saying which call this was.
    fn describe_request(&self) -> String {
        format!("{} {}", self.method, self.path)
    }

    /// Every attempt this call is allowed, and whatever the last one produced.
    ///
    /// The budget bounds the whole of it. The retry loop stops short of a
    /// sleep that would run past the deadline, which is the tidy way out and
    /// leaves the caller holding the failure that caused the wait; the timeout
    /// around the loop is the untidy one, for an attempt that is still going
    /// when the budget is already spent.
    async fn send_raw(self) -> Result<RawBody, ApiError> {
        let request = self.describe_request();
        let budget = self.budget;
        match tokio::time::timeout(budget, self.attempts()).await {
            Ok(answer) => answer,
            Err(_) => Err(ApiError::Timeout { request, budget }),
        }
    }

    /// The retry loop itself, bounded from outside by [`Self::send_raw`].
    async fn attempts(self) -> Result<RawBody, ApiError> {
        if let Some(broken) = self.broken {
            return Err(broken);
        }
        let request = self.describe_request();
        let idempotence = idempotence(&self.method);
        let deadline = Instant::now() + self.budget;
        let inner = &self.client.inner;
        let url = format!("{}{}", inner.base_url, self.path);

        let mut attempt = 0;
        let mut refreshed = false;
        loop {
            attempt += 1;
            let outcome = self.attempt(&url, &request).await;
            let error = match outcome {
                Ok(answer) => return Ok(answer),
                Err(error) => error,
            };

            // A refused token is its own kind of retry, and a short one: the
            // attempt evicted it, so going round again mints another and sends
            // that. Once per call, because a second 401 on a fresh token is
            // the credential being wrong rather than the token being stale,
            // and no method is at risk — a 401 means nothing was done.
            if error.status() == Some(UNAUTHORIZED)
                && inner.tokens.can_refresh()
                && !refreshed
                && attempt < MAX_ATTEMPTS
            {
                refreshed = true;
                tracing::debug!(request = %request, "the token was refused; minting another");
                continue;
            }

            // Two conditions, and both have to hold. `is_transient` is about
            // the failure — would asking again plausibly work — and this is
            // about the request: a `POST` that may have been acted on before
            // the answer went missing must not be sent twice. A 429 is the
            // exception, and only because it means the server declined to act.
            let repeatable =
                idempotence == Idempotence::Repeatable || error.status() == Some(TOO_MANY_REQUESTS);
            if !error.is_transient() || !repeatable || attempt >= MAX_ATTEMPTS {
                return Err(error);
            }

            let delay = backoff(attempt, &error);
            if Instant::now() + delay >= deadline {
                // Sleeping past the budget would turn a described failure into
                // a bare timeout. The caller is better told what went wrong.
                return Err(error);
            }
            tracing::debug!(
                request = %request,
                attempt,
                delay_ms = delay.as_millis(),
                because = %error,
                "retrying a control-plane call"
            );
            tokio::time::sleep(delay).await;
        }
    }

    /// One attempt: a permit, a token, a request, and an answer read under the
    /// size cap.
    async fn attempt(&self, url: &str, request: &str) -> Result<RawBody, ApiError> {
        let inner = &self.client.inner;
        let _permit = inner
            .in_flight
            .acquire()
            .await
            .map_err(|_| ApiError::Config("the client has been shut down".to_owned()))?;

        let bearer = inner.tokens.bearer().await?;
        let mut sending = inner
            .http
            .request(self.method.clone(), url)
            .bearer_auth(bearer.value.expose())
            .query(&self.query);
        for (name, value) in &self.headers {
            sending = sending.header(name, value);
        }
        match &self.body {
            Some(Body::Json(value)) => sending = sending.json(value),
            Some(Body::Text { content_type, text }) => {
                sending = sending
                    .header(reqwest::header::CONTENT_TYPE, content_type)
                    .body(text.clone());
            }
            None => {}
        }

        let response = sending.send().await.map_err(|source| {
            if source.is_timeout() {
                ApiError::Timeout {
                    request: request.to_owned(),
                    budget: self.budget,
                }
            } else {
                ApiError::Transport {
                    request: request.to_owned(),
                    source,
                }
            }
        })?;

        let status = response.status();
        if status.is_success() {
            return read_body(response, request, inner.max_response_bytes).await;
        }

        if status.as_u16() == UNAUTHORIZED
            && let Some(generation) = bearer.generation
        {
            // The token was refused, so the next attempt must not send it
            // again. Doing this by generation is what keeps one rejection from
            // throwing away several freshly minted tokens in a row.
            inner.tokens.evict(generation).await;
        }

        let retry_after = retry_after(&response);
        let body = read_body(response, request, MAX_ERROR_BYTES)
            .await
            .map(|raw| String::from_utf8_lossy(&raw.bytes).into_owned())
            .unwrap_or_default();
        Err(ApiError::Status {
            request: request.to_owned(),
            status: status.as_u16(),
            message: describe(status, &body),
            retry_after,
        })
    }
}

/// A body, read whole.
#[derive(Debug)]
struct RawBody {
    bytes: Vec<u8>,
    etag: Option<String>,
}

/// A body, parsed. Empty means nothing was said, not that nothing parsed.
///
/// One function rather than three copies of the same two lines, so that the
/// three `send` methods cannot come to disagree about what an empty body is.
fn parse(bytes: &[u8], request: &str) -> Result<Value, ApiError> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Null);
    }
    serde_json::from_slice(bytes).map_err(|source| ApiError::Malformed {
        request: request.to_owned(),
        source,
    })
}

/// A parsed answer, and the body it was parsed from.
///
/// Both halves are kept because both are used: `raw` is what a tool hands
/// back, so the caller sees whatever the control plane sent rather than the
/// subset this build knows the names of, and `value` is what the server reads
/// when it has to act on the answer.
#[derive(Debug, Clone)]
pub struct Answer<T> {
    pub value: T,
    /// The body, parsed as JSON and otherwise untouched.
    pub raw: Value,
    /// The `ETag` header, for the endpoints that version their document.
    pub etag: Option<String>,
}

/// A body that was asked for as text, and the version it was read at.
#[derive(Debug, Clone)]
pub struct TextBody {
    pub text: String,
    /// The `ETag` header, which the policy file's writes quote back.
    pub etag: Option<String>,
}

/// Read a body, refusing rather than truncating one that is too large.
///
/// Truncating would be worse than failing: half a JSON document does not parse,
/// and half a device list that does parse is a wrong answer nobody can see is
/// wrong. The cap is checked against `Content-Length` first so an enormous
/// answer is refused before it is transferred, and again while reading, because
/// a chunked response does not have one.
async fn read_body(
    mut response: reqwest::Response,
    request: &str,
    cap: usize,
) -> Result<RawBody, ApiError> {
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    let too_large = || ApiError::TooLarge {
        request: request.to_owned(),
        cap,
    };
    if response
        .content_length()
        .is_some_and(|len| len > cap as u64)
    {
        return Err(too_large());
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|source| ApiError::Transport {
            request: request.to_owned(),
            source,
        })?
    {
        if bytes.len() + chunk.len() > cap {
            return Err(too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(RawBody { bytes, etag })
}

/// HTTP's own answer to whether a request may be sent twice.
///
/// `GET`, `HEAD`, `PUT` and `DELETE` are defined to be idempotent and `POST`
/// and `PATCH` are not, so this needs no table of its own: minting an auth key
/// is a `POST` because minting twice is two keys.
fn idempotence(method: &reqwest::Method) -> Idempotence {
    match *method {
        reqwest::Method::POST | reqwest::Method::PATCH => Idempotence::Once,
        _ => Idempotence::Repeatable,
    }
}

/// How long to wait before attempt number `attempt + 1`.
///
/// The server's own `Retry-After` wins where it sent one: it knows when its
/// limit resets and this side is guessing. Everything else doubles from
/// [`BASE_BACKOFF`]. There is no jitter, because there is one client here and
/// nobody to collide with.
fn backoff(attempt: u32, error: &ApiError) -> Duration {
    if let ApiError::Status {
        retry_after: Some(asked),
        ..
    } = error
    {
        return (*asked).min(MAX_BACKOFF);
    }
    (BASE_BACKOFF * 2u32.saturating_pow(attempt - 1)).min(MAX_BACKOFF)
}

/// The `Retry-After` header, in the seconds form the API sends.
///
/// The HTTP-date form is legal and Tailscale does not use it; reading a wrong
/// number out of a date would be worse than falling back to the backoff.
fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Where a client may point.
///
/// A base URL is where every credential this server holds gets sent, so the
/// three things asked of one are the three that keep it from being a way to
/// send them somewhere in the clear: the transport is `https`, or the host is
/// this machine, which is how the fake in this crate is reached; there is no
/// path, because a base URL is a host and nothing more; and there is no
/// userinfo, because a credential in a URL is a credential that gets printed.
///
/// What this does not do is name the host. `https://api.tailscale.com` is the
/// default, and the guarantee here is about how a credential travels rather
/// than about where it lands.
pub fn checked_base_url(base_url: &str) -> Result<String, ApiError> {
    let trimmed = base_url.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|source| ApiError::Config(format!("`{base_url}` is not a URL: {source}")))?;

    let loopback = parsed.host_str().is_some_and(|host| {
        // `host_str` keeps an IPv6 address in the brackets the URL form
        // requires, and `IpAddr` does not parse those.
        let host = host.trim_start_matches('[').trim_end_matches(']');
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    if parsed.scheme() != "https" && !loopback {
        return Err(ApiError::Config(format!(
            "`{base_url}` is neither https nor a loopback address, and a \
             control-plane credential is not sent anywhere else"
        )));
    }
    if !parsed.path().is_empty() && parsed.path() != "/" {
        return Err(ApiError::Config(format!(
            "`{base_url}` has a path; the base URL is a host and nothing more"
        )));
    }
    // The URL is deliberately not echoed back here: the objection to userinfo
    // is that it is a secret in a place secrets get printed, and this message
    // is printed.
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ApiError::Config(
            "the base URL carries a username or password; a control-plane \
             credential is sent as a header and never in a URL"
                .to_owned(),
        ));
    }
    Ok(trimmed.to_owned())
}

/// One path segment, with everything that is not plainly safe escaped.
///
/// A tailnet is named by a domain and a device by an opaque identifier, so in
/// practice nothing here needs escaping. The point is the case where something
/// does: a `/` in a name would otherwise be a segment boundary, and a name is
/// not always chosen by the person the path is built for.
fn escape(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'@') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::fake::{FakeControlPlane, Response};
    use crate::secret::Secret;

    /// A key that is obviously not one.
    const KEY: &str = "tskey-api-redacted-example";
    const DEVICES: &str = "/api/v2/tailnet/-/devices";

    async fn fake() -> FakeControlPlane {
        FakeControlPlane::start()
            .await
            .expect("a loopback socket is available")
    }

    fn client(fake: &FakeControlPlane, credentials: Credentials) -> Client {
        client_with(fake, credentials, |_| {})
    }

    fn client_with(
        fake: &FakeControlPlane,
        credentials: Credentials,
        adjust: impl FnOnce(&mut ClientConfig),
    ) -> Client {
        let mut config = ClientConfig::new(credentials);
        config.base_url = fake.base_url().to_owned();
        adjust(&mut config);
        Client::new(config).expect("the fake answers on a loopback address")
    }

    fn api_key() -> Credentials {
        Credentials::ApiKey(Secret::new(KEY))
    }

    fn oauth() -> Credentials {
        Credentials::OauthClient {
            client_id: "kExAmPlE1CNTRL".to_owned(),
            client_secret: Secret::new("tskey-client-redacted-example"),
            scopes: vec!["devices:read".to_owned(), "dns".to_owned()],
        }
    }

    /// A token endpoint answer worth `seconds`.
    fn token(value: &str, seconds: u64) -> Response {
        Response::json(json!({
            "access_token": value,
            "token_type": "Bearer",
            "expires_in": seconds,
        }))
    }

    /// Every `Authorization` header that arrived, in order.
    fn bearers(fake: &FakeControlPlane) -> Vec<String> {
        fake.recorded()
            .into_iter()
            .filter_map(|r| r.authorization().map(str::to_owned))
            .collect()
    }

    // ---- authentication -------------------------------------------------

    #[tokio::test]
    async fn an_api_key_is_the_bearer_token_itself() {
        let fake = fake()
            .await
            .on("GET", DEVICES, Response::json(json!({"devices": []})));
        let client = client(&fake, api_key());

        let answer = client.get(DEVICES).send().await.expect("the fake answers");

        assert_eq!(answer, json!({"devices": []}));
        // One request, so nothing was exchanged: a key is already a token.
        let request = fake.only_request();
        assert_eq!(
            request.authorization(),
            Some(format!("Bearer {KEY}").as_str())
        );
    }

    #[tokio::test]
    async fn an_oauth_client_is_exchanged_for_a_token() {
        let fake = fake()
            .await
            .on("POST", crate::token::TOKEN_PATH, token("minted-1", 3600))
            .on("GET", DEVICES, Response::json(json!({"devices": []})));
        let client = client(&fake, oauth());

        client.get(DEVICES).send().await.expect("the fake answers");

        let recorded = fake.recorded();
        assert_eq!(
            recorded.len(),
            2,
            "an exchange and then the call: {recorded:#?}"
        );
        let exchange = &recorded[0];
        assert_eq!(exchange.path, crate::token::TOKEN_PATH);
        for expected in [
            "grant_type=client_credentials",
            "client_id=kExAmPlE1CNTRL",
            "client_secret=tskey-client-redacted-example",
            // OAuth spells a scope list space-separated whatever separator the
            // environment variable used.
            "scope=devices%3Aread+dns",
        ] {
            assert!(
                exchange.body.contains(expected),
                "the exchange did not send `{expected}`: {}",
                exchange.body
            );
        }
        assert_eq!(recorded[1].authorization(), Some("Bearer minted-1"));
    }

    #[tokio::test]
    async fn a_federated_identity_signs_with_the_jwt_on_disk() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let jwt_file = directory.path().join("token");
        std::fs::write(&jwt_file, "header.payload.signature\n").expect("the file is written");

        let fake = fake()
            .await
            .on(
                "POST",
                crate::token::TOKEN_PATH,
                token("minted-federated", 3600),
            )
            .on("GET", DEVICES, Response::json(json!({"devices": []})));
        let client = client(
            &fake,
            Credentials::Federated {
                client_id: Some("kExAmPlE1CNTRL".to_owned()),
                jwt_file,
                scopes: Vec::new(),
            },
        );

        client.get(DEVICES).send().await.expect("the fake answers");

        let exchange = &fake.recorded()[0];
        assert!(
            exchange
                .body
                .contains("client_assertion=header.payload.signature"),
            "the JWT was not sent, or was sent with its trailing newline: {}",
            exchange.body
        );
        assert!(
            exchange.body.contains("client_assertion_type=urn%3Aietf"),
            "the assertion type was not sent: {}",
            exchange.body
        );
    }

    #[tokio::test]
    async fn a_missing_jwt_file_says_which_file_it_was() {
        let fake = fake().await;
        let client = client(
            &fake,
            Credentials::Federated {
                client_id: None,
                jwt_file: PathBuf::from("/nonexistent/identity/token"),
                scopes: Vec::new(),
            },
        );

        let error = client
            .get(DEVICES)
            .send()
            .await
            .expect_err("there is no file");

        assert!(
            matches!(&error, ApiError::JwtFile { path, .. } if path.ends_with("token")),
            "unexpected error: {error:?}"
        );
        assert_eq!(fake.request_count(), 0, "nothing should have been sent");
    }

    #[tokio::test]
    async fn the_credential_with_precedence_is_the_one_that_is_used() {
        // Both are set, which is what an operator with an old key in a shell
        // profile looks like. The key wins, so no exchange happens at all.
        let environment = |key: &str| match key {
            crate::credentials::API_KEY_ENV => Some(KEY.to_owned()),
            crate::credentials::OAUTH_CLIENT_ID_ENV => Some("kExAmPlE1CNTRL".to_owned()),
            crate::credentials::OAUTH_CLIENT_SECRET_ENV => Some("unused".to_owned()),
            _ => None,
        };
        let credentials = Credentials::from_source(environment).expect("both are set");

        let fake = fake().await.on("GET", DEVICES, Response::json(json!({})));
        let client = client(&fake, credentials);
        client.get(DEVICES).send().await.expect("the fake answers");

        let request = fake.only_request();
        assert_eq!(
            request.authorization(),
            Some(format!("Bearer {KEY}").as_str())
        );
    }

    // ---- the token's life ------------------------------------------------

    #[tokio::test]
    async fn a_token_is_minted_once_and_reused() {
        let fake = fake()
            .await
            .on("POST", crate::token::TOKEN_PATH, token("minted-1", 3600))
            .on("GET", DEVICES, Response::json(json!({})));
        let client = client(&fake, oauth());

        for _ in 0..3 {
            client.get(DEVICES).send().await.expect("the fake answers");
        }

        let exchanges = fake
            .recorded()
            .iter()
            .filter(|r| r.path == crate::token::TOKEN_PATH)
            .count();
        assert_eq!(exchanges, 1, "the token should have been minted once");
        // The exchange itself carries no bearer — it is how one is obtained —
        // so the three calls are the three headers.
        assert_eq!(bearers(&fake), vec!["Bearer minted-1".to_owned(); 3]);
    }

    #[tokio::test]
    async fn a_token_near_its_expiry_is_minted_again() {
        // Half a minute of life left, which is a token the clock still calls
        // valid: only the refresh skew makes this one spent. A token that had
        // already expired would be re-minted with no skew at all, and so would
        // prove nothing about the last minute this deliberately gives up.
        let remaining = crate::token::REFRESH_SKEW.as_secs() / 2;
        let fake = fake()
            .await
            .on(
                "POST",
                crate::token::TOKEN_PATH,
                token("minted-1", remaining),
            )
            .on("GET", DEVICES, Response::json(json!({})));
        let client = client(&fake, oauth());

        for _ in 0..2 {
            client.get(DEVICES).send().await.expect("the fake answers");
        }

        let exchanges = fake
            .recorded()
            .iter()
            .filter(|r| r.path == crate::token::TOKEN_PATH)
            .count();
        assert_eq!(exchanges, 2, "a token inside the skew should not be reused");
    }

    #[tokio::test]
    async fn a_refused_token_is_replaced_exactly_once() {
        let fake = fake()
            .await
            .once("POST", crate::token::TOKEN_PATH, token("stale", 3600))
            .on("POST", crate::token::TOKEN_PATH, token("fresh", 3600))
            .once(
                "GET",
                DEVICES,
                Response::status(401, json!({"message": "expired"})),
            )
            .on("GET", DEVICES, Response::json(json!({"devices": []})));
        let client = client(&fake, oauth());

        let answer = client
            .get(DEVICES)
            .send()
            .await
            .expect("the second try works");

        assert_eq!(answer, json!({"devices": []}));
        assert_eq!(
            bearers(&fake),
            vec!["Bearer stale".to_owned(), "Bearer fresh".to_owned()],
            "the refused token should have been replaced, once"
        );
    }

    #[tokio::test]
    async fn a_token_refused_twice_is_the_credential_being_wrong() {
        let fake = fake()
            .await
            .on("POST", crate::token::TOKEN_PATH, token("minted", 3600))
            .on(
                "GET",
                DEVICES,
                Response::status(401, json!({"message": "no"})),
            );
        let client = client(&fake, oauth());

        let error = client
            .get(DEVICES)
            .send()
            .await
            .expect_err("it is always refused");

        assert_eq!(error.status(), Some(401));
        let calls = fake.recorded().iter().filter(|r| r.path == DEVICES).count();
        assert_eq!(calls, 2, "one retry with a fresh token, and then no more");
    }

    #[tokio::test]
    async fn a_refused_api_key_is_not_replaced_because_there_is_nothing_to_mint() {
        let fake = fake().await.on(
            "GET",
            DEVICES,
            Response::status(401, json!({"message": "no"})),
        );
        let client = client(&fake, api_key());

        let error = client.get(DEVICES).send().await.expect_err("it is refused");

        assert_eq!(error.status(), Some(401));
        assert_eq!(fake.request_count(), 1);
    }

    // ---- retry -----------------------------------------------------------

    #[tokio::test]
    async fn a_transient_failure_on_a_repeatable_method_is_retried() {
        let fake = fake()
            .await
            .once(
                "GET",
                DEVICES,
                Response::status(503, json!({"message": "later"})),
            )
            .on("GET", DEVICES, Response::json(json!({"devices": []})));
        let client = client(&fake, api_key());

        let answer = client
            .get(DEVICES)
            .send()
            .await
            .expect("the second try works");

        assert_eq!(answer, json!({"devices": []}));
        assert_eq!(fake.request_count(), 2);
    }

    #[tokio::test]
    async fn a_transient_failure_on_an_unsafe_method_is_not_retried() {
        // The point of the rule: this is the shape of minting an auth key,
        // where a retry is a second key nobody asked for and nobody sees.
        let keys = "/api/v2/tailnet/-/keys";
        let fake = fake().await.on(
            "POST",
            keys,
            Response::status(503, json!({"message": "later"})),
        );
        let client = client(&fake, api_key());

        let error = client
            .post(keys)
            .json(&json!({"capabilities": {}}))
            .send()
            .await
            .expect_err("the fake never succeeds");

        assert_eq!(error.status(), Some(503));
        assert_eq!(fake.request_count(), 1, "a POST must not be sent twice");
    }

    #[tokio::test]
    async fn a_rate_limit_is_retried_even_on_an_unsafe_method() {
        // A 429 says the server declined to act, so nothing happened and the
        // reason not to repeat a POST does not apply.
        let keys = "/api/v2/tailnet/-/keys";
        let fake = fake()
            .await
            .once(
                "POST",
                keys,
                Response::status(429, json!({"message": "slow down"}))
                    .with_header("retry-after", "0"),
            )
            .on(
                "POST",
                keys,
                Response::json(json!({"key": "tskey-auth-redacted-example"})),
            );
        let client = client(&fake, api_key());

        let answer = client
            .post(keys)
            .json(&json!({"capabilities": {}}))
            .send()
            .await
            .expect("the second try works");

        assert_eq!(answer["key"], json!("tskey-auth-redacted-example"));
        assert_eq!(fake.request_count(), 2);
    }

    #[tokio::test]
    async fn a_permanent_failure_is_not_retried() {
        let fake = fake().await.on(
            "GET",
            DEVICES,
            Response::status(404, json!({"message": "no such tailnet"})),
        );
        let client = client(&fake, api_key());

        let error = client
            .get(DEVICES)
            .send()
            .await
            .expect_err("there is nothing there");

        assert!(
            matches!(&error, ApiError::Status { message, .. } if message == "no such tailnet"),
            "the API's own message should be passed on: {error:?}"
        );
        assert_eq!(fake.request_count(), 1);
    }

    #[tokio::test]
    async fn a_call_stops_after_a_bounded_number_of_attempts() {
        let fake = fake().await.on(
            "GET",
            DEVICES,
            Response::status(503, json!({"message": "later"})).with_header("retry-after", "0"),
        );
        let client = client(&fake, api_key());

        let error = client
            .get(DEVICES)
            .send()
            .await
            .expect_err("it never works");

        assert_eq!(error.status(), Some(503));
        assert_eq!(fake.request_count(), MAX_ATTEMPTS as usize);
    }

    #[tokio::test]
    async fn retrying_stops_when_the_budget_would_not_cover_the_wait() {
        // The budget is the tool's timeout. Sleeping past it would turn a
        // failure that says what went wrong into a bare timeout.
        let fake = fake().await.on(
            "GET",
            DEVICES,
            Response::status(503, json!({"message": "later"})),
        );
        let client = client(&fake, api_key());

        let error = client
            .get(DEVICES)
            .budget(Duration::from_millis(50))
            .send()
            .await
            .expect_err("it never works");

        assert_eq!(error.status(), Some(503), "not a bare timeout: {error:?}");
        assert_eq!(
            fake.request_count(),
            1,
            "the first backoff is longer than the budget"
        );
    }

    #[tokio::test]
    async fn the_wait_a_server_asks_for_is_read_off_the_wire() {
        // `the_server_is_believed_about_when_to_come_back` builds the header's
        // value by hand and so proves only what `backoff` does with it. This
        // is the other half: the same status and the same budget, differing in
        // nothing but the header, produce different numbers of requests, which
        // they can only do if the number was read from the response.
        //
        // Both halves finish in the time one request takes. Five minutes is
        // clamped to `MAX_BACKOFF`, which is still longer than the budget, so
        // the call gives up rather than sleeping; zero is a wait of nothing.
        let budget = Duration::from_secs(1);
        let refusal = |wait| {
            Response::status(503, json!({"message": "later"})).with_header("retry-after", wait)
        };

        let patient = fake().await.on("GET", DEVICES, refusal("300"));
        let error = client(&patient, api_key())
            .get(DEVICES)
            .budget(budget)
            .send()
            .await
            .expect_err("it never works");
        assert_eq!(error.status(), Some(503), "not a bare timeout: {error:?}");
        assert_eq!(
            patient.request_count(),
            1,
            "the server asked for longer than the budget, so there was no second try; \
             ignoring the header would have waited {BASE_BACKOFF:?} and tried again"
        );

        let impatient = fake().await.on("GET", DEVICES, refusal("0"));
        client(&impatient, api_key())
            .get(DEVICES)
            .budget(budget)
            .send()
            .await
            .expect_err("it never works");
        assert_eq!(
            impatient.request_count(),
            MAX_ATTEMPTS as usize,
            "a server asking for no wait at all should be believed too"
        );
    }

    #[test]
    fn the_server_is_believed_about_when_to_come_back() {
        let asked = |seconds| ApiError::Status {
            request: "GET /x".to_owned(),
            status: 429,
            message: String::new(),
            retry_after: Some(Duration::from_secs(seconds)),
        };
        // A 503 with no header, which is the ordinary case.
        let guessed = ApiError::Status {
            request: "GET /x".to_owned(),
            status: 503,
            message: String::new(),
            retry_after: None,
        };

        assert_eq!(backoff(1, &asked(5)), Duration::from_secs(5));
        // A server asking for longer than any call budget is asking for the
        // budget to be spent asleep.
        assert_eq!(backoff(1, &asked(600)), MAX_BACKOFF);
        // Without a header, the wait doubles and then stops growing.
        assert_eq!(backoff(1, &guessed), BASE_BACKOFF);
        assert_eq!(backoff(2, &guessed), BASE_BACKOFF * 2);
        assert_eq!(backoff(30, &guessed), MAX_BACKOFF);
    }

    #[test]
    fn only_the_methods_http_calls_idempotent_may_be_repeated() {
        for method in [
            reqwest::Method::GET,
            reqwest::Method::HEAD,
            reqwest::Method::PUT,
            reqwest::Method::DELETE,
        ] {
            assert_eq!(idempotence(&method), Idempotence::Repeatable, "{method}");
        }
        for method in [reqwest::Method::POST, reqwest::Method::PATCH] {
            assert_eq!(idempotence(&method), Idempotence::Once, "{method}");
        }
    }

    // ---- concurrency -----------------------------------------------------

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn no_more_calls_are_in_flight_than_the_limit_allows() {
        const LIMIT: usize = 2;
        let fake = fake().await.on(
            "GET",
            DEVICES,
            Response::json(json!({})).slow(Duration::from_millis(80)),
        );
        let client = client_with(&fake, api_key(), |config| config.concurrency = LIMIT);

        let calls: Vec<_> = (0..8)
            .map(|_| {
                let client = client.clone();
                tokio::spawn(async move { client.get(DEVICES).send().await })
            })
            .collect();
        for call in calls {
            call.await
                .expect("the task finished")
                .expect("the fake answers");
        }

        assert_eq!(fake.request_count(), 8);
        let peak = fake.peak_concurrency();
        assert!(
            (1..=LIMIT).contains(&peak),
            "{peak} calls were in flight at once, and the limit is {LIMIT}"
        );
    }

    // ---- the size cap ----------------------------------------------------

    #[tokio::test]
    async fn an_answer_over_the_cap_is_refused_rather_than_truncated() {
        let big = json!({"devices": vec![json!({"name": "x".repeat(200)})]});
        let fake = fake().await.on("GET", DEVICES, Response::json(&big));
        let client = client_with(&fake, api_key(), |config| config.max_response_bytes = 64);

        let error = client
            .get(DEVICES)
            .send()
            .await
            .expect_err("it is too large");

        assert!(
            matches!(error, ApiError::TooLarge { cap: 64, .. }),
            "a truncated body would have failed to parse instead: {error:?}"
        );
    }

    #[tokio::test]
    async fn an_answer_with_no_stated_length_is_refused_while_it_is_read() {
        // Chunked, so the cap cannot be checked before the transfer and has to
        // hold while the body arrives.
        let big = json!({"devices": vec![json!({"name": "x".repeat(200)})]});
        let fake = fake()
            .await
            .on("GET", DEVICES, Response::json(&big).chunked());
        let client = client_with(&fake, api_key(), |config| config.max_response_bytes = 64);

        let error = client
            .get(DEVICES)
            .send()
            .await
            .expect_err("it is too large");

        assert!(
            matches!(error, ApiError::TooLarge { cap: 64, .. }),
            "unexpected error: {error:?}"
        );
    }

    #[tokio::test]
    async fn an_answer_under_the_cap_arrives_whole_however_it_is_framed() {
        let body = json!({"devices": [{"name": "workstation"}]});
        let fake = fake()
            .await
            .on("GET", DEVICES, Response::json(&body).chunked());
        let client = client(&fake, api_key());

        let answer = client.get(DEVICES).send().await.expect("the fake answers");

        assert_eq!(answer, body);
    }

    // ---- shapes of a call ------------------------------------------------

    #[tokio::test]
    async fn an_empty_body_is_an_answer_rather_than_a_parse_failure() {
        let device = "/api/v2/device/n1111111CNTRL";
        let fake = fake().await.on("DELETE", device, Response::empty());
        let client = client(&fake, api_key());

        let answer = client
            .delete(device)
            .send()
            .await
            .expect("the fake answers");

        assert_eq!(answer, Value::Null, "a deletion answers with nothing");
    }

    #[tokio::test]
    async fn an_answer_is_read_both_ways_from_one_parse() {
        // The reason `Answer` holds both halves (ADR-0003): a tool forwards
        // `raw` so the caller sees every field the control plane sent, and the
        // server reads `value` when it has to act. They come from one parse, so
        // a field in one is a field in the other.
        let body = json!({
            "id": "kExAmPlE",
            "description": "a key",
            "invented": {"by": "a later control plane"},
        });
        let keys = "/api/v2/tailnet/-/keys/kExAmPlE";
        let fake = fake().await.on("GET", keys, Response::json(&body));
        let client = client(&fake, api_key());

        let answer = client
            .get(keys)
            .send_answer::<crate::models::key::Key>()
            .await
            .expect("the fake answers");

        assert_eq!(answer.value.id.as_deref(), Some("kExAmPlE"));
        assert_eq!(
            answer.value.unknown.get("invented"),
            Some(&json!({"by": "a later control plane"})),
            "the typed half keeps what it had no field for"
        );
        assert_eq!(answer.raw, body, "and the raw half is the body, untouched");
    }

    #[tokio::test]
    async fn an_answer_carries_the_etag_that_versions_it() {
        // The other thing ADR-0003 asks `Answer` to carry. The policy file is
        // the endpoint that needs it: a write has to quote the `ETag` it read.
        let acl = "/api/v2/tailnet/-/acl";
        let fake = fake().await.on(
            "GET",
            acl,
            Response::json(json!({"acls": []})).with_header("ETag", "\"abc123\""),
        );
        let client = client(&fake, api_key());

        let answer = client
            .get(acl)
            .send_answer::<Value>()
            .await
            .expect("the fake answers");

        assert_eq!(answer.etag.as_deref(), Some("\"abc123\""));
    }

    #[tokio::test]
    async fn an_empty_body_answers_as_nothing_rather_than_failing_to_parse() {
        // What a deletion sends, read through `send_answer` rather than
        // `send`. `Value` reads null; a model could not, which is documented
        // on `send_answer` and is why deletions ask for `Value`.
        let device = "/api/v2/device/n1111111CNTRL";
        let fake = fake().await.on("DELETE", device, Response::empty());
        let client = client(&fake, api_key());

        let answer = client
            .delete(device)
            .send_answer::<Value>()
            .await
            .expect("the fake answers");

        assert_eq!(answer.value, Value::Null);
        assert_eq!(answer.raw, Value::Null, "both halves agree about nothing");
    }

    #[tokio::test]
    async fn the_query_and_the_body_reach_the_control_plane_as_written() {
        let fake = fake().await.on("POST", DEVICES, Response::json(json!({})));
        let client = client(&fake, api_key());

        client
            .post(DEVICES)
            .query("fields", "all")
            .maybe_query("since", Some(7))
            .maybe_query("until", Option::<u8>::None)
            .header("If-Match", "\"v1\"")
            .json(&json!({"name": "workstation"}))
            .send()
            .await
            .expect("the fake answers");

        let request = fake.only_request();
        assert_eq!(
            request
                .query
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["fields", "since"]),
            "an absent parameter should not be sent"
        );
        assert_eq!(request.query["fields"], "all");
        assert_eq!(
            request.headers.get("if-match").map(String::as_str),
            Some("\"v1\"")
        );
        assert_eq!(request.json(), json!({"name": "workstation"}));
    }

    #[tokio::test]
    async fn text_comes_back_with_the_version_it_was_read_at() {
        // The policy file is HuJSON, which is not JSON to parse, and its ETag
        // is what a later write has to quote.
        let policy = "/api/v2/tailnet/-/acl";
        let hujson = "{\n  // a comment, which JSON does not have\n  \"acls\": [],\n}";
        let fake = fake().await.on(
            "GET",
            policy,
            Response {
                status: 200,
                headers: vec![("content-type".to_owned(), "application/hujson".to_owned())],
                body: hujson.to_owned(),
                delay: Duration::ZERO,
                chunked: false,
            }
            .with_header("etag", "\"abc123\""),
        );
        let client = client(&fake, api_key());

        let answer = client
            .get(policy)
            .send_text()
            .await
            .expect("the fake answers");

        assert_eq!(answer.text, hujson);
        assert_eq!(answer.etag.as_deref(), Some("\"abc123\""));
    }

    #[tokio::test]
    async fn a_body_that_is_not_what_was_asked_for_says_so() {
        #[derive(Debug, serde::Deserialize)]
        struct Devices {
            #[allow(dead_code)]
            devices: Vec<String>,
        }
        let fake = fake()
            .await
            .on("GET", DEVICES, Response::json(json!({"devices": 7})));
        let client = client(&fake, api_key());

        let error = client
            .get(DEVICES)
            .send_as::<Devices>()
            .await
            .expect_err("seven is not a list");

        assert!(
            matches!(&error, ApiError::Malformed { request, .. } if request == "GET /api/v2/tailnet/-/devices"),
            "unexpected error: {error:?}"
        );
    }

    // ---- where a client may point ----------------------------------------

    #[test]
    fn a_base_url_is_an_encrypted_host_and_nothing_more() {
        // Deliberately not "…is the control plane": the host is not pinned,
        // and a name that said so would be describing a guarantee this does
        // not make. What is checked is how the credential travels, which is
        // the three things below.
        for allowed in [
            DEFAULT_BASE_URL,
            "https://api.example.com",
            "https://example.com",
            "http://127.0.0.1:8080",
            "http://localhost:9999",
            "http://[::1]:1234",
        ] {
            assert!(
                checked_base_url(allowed).is_ok(),
                "{allowed} should have been accepted"
            );
        }
        for refused in [
            // Plaintext to anywhere but this machine sends the credential in
            // the clear.
            "http://api.tailscale.com",
            "http://evil.example.com",
            // Not a URL, and a scheme that is not HTTP at all.
            "api.tailscale.com",
            "ftp://api.tailscale.com",
            // A base URL is a host; a path here would silently prefix every
            // call, which is a different server wearing the same name.
            "https://api.tailscale.com/api/v2",
            // Userinfo is a secret written where secrets get printed, and
            // this server sends its credential as a header regardless. The
            // host is `example.com`, which is accepted bare just above, so
            // userinfo is the only thing these two differ by.
            "https://user:pass@example.com",
            "https://token@example.com",
        ] {
            assert!(
                checked_base_url(refused).is_err(),
                "{refused} should have been refused"
            );
        }
        // A trailing slash is how a URL is usually written down, and joining
        // it to a path that starts with one would double it.
        assert_eq!(
            checked_base_url("https://api.tailscale.com/").expect("a valid URL"),
            DEFAULT_BASE_URL
        );
    }

    #[test]
    fn a_name_in_a_path_cannot_reach_into_the_path_around_it() {
        let fake_config = ClientConfig::new(api_key());
        let client = Client::new(fake_config).expect("the default base URL is valid");

        assert_eq!(
            client.tailnet_path(None, "/devices"),
            "/api/v2/tailnet/-/devices"
        );
        assert_eq!(
            client.tailnet_path(Some("example.com"), "/dns/nameservers"),
            "/api/v2/tailnet/example.com/dns/nameservers"
        );
        // An empty name means "not given" rather than an empty segment.
        assert_eq!(
            client.tailnet_path(Some("  "), "/devices"),
            "/api/v2/tailnet/-/devices"
        );
        // The point: a slash in a name is a character, not a boundary.
        assert_eq!(
            client.tailnet_path(Some("../../device/n1111111CNTRL"), "/devices"),
            "/api/v2/tailnet/..%2F..%2Fdevice%2Fn1111111CNTRL/devices"
        );
    }

    #[test]
    fn a_client_that_could_not_work_is_refused_at_the_start() {
        for (what, adjust) in [
            (
                "no calls in flight",
                Box::new(|c: &mut ClientConfig| c.concurrency = 0) as Box<dyn FnOnce(&mut _)>,
            ),
            (
                "no bytes allowed back",
                Box::new(|c: &mut ClientConfig| c.max_response_bytes = 0),
            ),
        ] {
            let mut config = ClientConfig::new(api_key());
            adjust(&mut config);
            assert!(
                matches!(Client::new(config), Err(ApiError::Config(_))),
                "{what} should have been refused"
            );
        }
    }
}
