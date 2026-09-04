//! The Streamable HTTP transport, and everything that stands in front of it.
//!
//! Stdio has one client, reached over a pipe the operating system already
//! decided who may open. HTTP has none of that: anything that can reach the
//! socket can try, and a browser on the same machine can be made to try by a
//! page the operator never visited. So the transport is the small part of this
//! module and the checks are the rest.
//!
//! **What has to be true before a request reaches the handler**, in the order
//! it is asked:
//!
//! 1. The `Host` header names something on the allow-list. Loopback, plus this
//!    node's own tailnet names read from status at startup, plus whatever the
//!    operator added. This is what stops DNS rebinding: a page that resolves
//!    `evil.example` to `127.0.0.1` reaches the socket and arrives with the
//!    wrong `Host`.
//! 2. There is no `Origin`, or the `Origin` is on its own allow-list. A
//!    request carrying one came from a page, and a page is not a client this
//!    server has any reason to serve unless the operator said so.
//! 3. The caller's address is under its rate limit.
//! 4. The bearer token matches, compared in constant time.
//!
//! The body limit is the one check that is not here: rmcp's transport reads
//! the body, so rmcp's transport is what caps it. See [`MAX_BODY_BYTES`].
//!
//! `GET /health` skips all four: it exists to be reachable by something that
//! holds no credential, which is the whole point of a health check.
//!
//! **A token is optional on loopback and required everywhere else.** Binding
//! to an address other than loopback without one refuses to start, and
//! `--http-no-auth` is the only way past that — a flag rather than an
//! omission, so that serving an unauthenticated tailnet address is something
//! an operator did rather than something that happened.

use std::collections::{BTreeSet, HashMap};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::context::SelfIdentity;

/// Where `--http` listens when it is given no address.
///
/// Loopback, because anything else is a decision an operator should make
/// rather than inherit. The port is above the ports people already run things
/// on — 3000, 8000, 8080, 8443 — and is not registered to anything, so a
/// default that collides is unlikely (Q86).
pub const DEFAULT_BIND: &str = "127.0.0.1:8449";

/// The largest request body accepted.
///
/// A policy file is the biggest thing a client sends, and the largest real one
/// is a few hundred kilobytes; four megabytes leaves room for a tailnet far
/// larger than any that exists without letting an unauthenticated caller make
/// this server allocate for it.
///
/// Enforced by rmcp's own transport, which reads the body and so is the only
/// thing positioned to stop reading it. `axum::extract::DefaultBodyLimit` is
/// not: it sets an extension that axum's extractors consult, and the MCP path
/// is a service that takes the body for itself.
pub const MAX_BODY_BYTES: usize = 4 << 20;

/// How many requests one address may make in [`RATE_WINDOW`].
pub const RATE_BURST: u32 = 120;

/// The window the burst is counted over.
pub const RATE_WINDOW: Duration = Duration::from_secs(60);

/// The path that answers without a token.
pub const HEALTH_PATH: &str = "/health";

/// The path the MCP transport is served at.
pub const MCP_PATH: &str = "/mcp";

/// Everything the checks need, built once at startup.
#[derive(Debug, Clone)]
pub struct Guard {
    /// The bearer token, or `None` for a session that accepts any caller.
    token: Option<Arc<tailscale_rest::Secret>>,
    /// Host header values that may reach the handler, lowercased and without
    /// a port.
    hosts: Arc<BTreeSet<String>>,
    /// Origins that may reach the handler, exactly as a browser sends them.
    origins: Arc<BTreeSet<String>>,
    limiter: Arc<Mutex<RateLimiter>>,
    /// This node's peers by address, for naming a caller in the log.
    peers: Arc<HashMap<IpAddr, String>>,
}

/// Why a request was refused.
///
/// One type so that the refusals are written in one place and cannot drift
/// into four spellings of "no".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    UnknownHost,
    ForbiddenOrigin,
    RateLimited,
    BadToken,
}

impl Refusal {
    pub const fn status(self) -> StatusCode {
        match self {
            // Not 401: the caller's credential is not the problem, and a
            // browser told to authenticate would ask a person for a password
            // that would not help.
            Self::UnknownHost | Self::ForbiddenOrigin => StatusCode::FORBIDDEN,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::BadToken => StatusCode::UNAUTHORIZED,
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::UnknownHost => {
                "this server does not answer for that `Host`; add it with `--http-allow-host`"
            }
            Self::ForbiddenOrigin => {
                "this server does not answer requests from a browser page; add the origin with \
                 `--http-allow-origin` if that is what you meant"
            }
            Self::RateLimited => "too many requests from this address; wait and try again",
            Self::BadToken => "a bearer token is required and did not match",
        }
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        // A sentence, not a page: the caller is a program, and the sentence is
        // what a person reads out of its logs.
        (self.status(), format!("{}\n", self.message())).into_response()
    }
}

/// Who is calling, as far as this server can tell.
///
/// Deliberately more than the transport needs today. `spec.md` asks that the
/// per-request hook be "shaped so that identity-derived authorisation can be
/// added later without changing the transport", and a hook that carried only
/// an address would have to change shape the first time a rule wanted a name.
#[derive(Debug, Clone)]
pub struct Caller {
    /// Where the request came from.
    pub address: IpAddr,
    /// The tailnet name of the node at that address, when it is one of this
    /// node's peers.
    pub name: Option<String>,
}

impl Caller {
    /// What a log line calls this caller.
    pub fn describe(&self) -> String {
        match &self.name {
            Some(name) => format!("{name} ({})", self.address),
            None => self.address.to_string(),
        }
    }
}

impl Guard {
    /// Everything the checks need, from the settings and what status said.
    ///
    /// The settings are one argument rather than three so that the token, the
    /// hosts and the origins travel as what they are — one operator's answer
    /// about one transport — rather than as three lists a caller could pass in
    /// the wrong order.
    pub fn for_session(
        settings: &crate::config::HttpConfig,
        identity: &SelfIdentity,
        peers: HashMap<IpAddr, String>,
    ) -> Self {
        Self::new(
            settings.token.clone(),
            &settings.allow_hosts,
            &settings.allow_origins,
            identity,
            peers,
        )
    }

    /// Build the checks from what the operator asked for and what status said.
    ///
    /// The allow-list always contains loopback and `localhost` and this node's
    /// own names; an operator adding to it is adding, never replacing, because
    /// a list that could be narrowed to nothing is one an operator can lock
    /// themselves out with.
    pub fn new(
        token: Option<tailscale_rest::Secret>,
        extra_hosts: &[String],
        origins: &[String],
        identity: &SelfIdentity,
        peers: HashMap<IpAddr, String>,
    ) -> Self {
        let mut hosts: BTreeSet<String> = ["localhost", "127.0.0.1", "[::1]", "::1"]
            .iter()
            .map(|host| (*host).to_owned())
            .collect();
        // This node's own tailnet names, so that reaching the server over the
        // tailnet works without configuration — which is the case the HTTP
        // transport exists for.
        if let Some(dns_name) = &identity.dns_name {
            let full = dns_name.trim_end_matches('.').to_ascii_lowercase();
            if let Some(short) = full.split('.').next() {
                hosts.insert(short.to_owned());
            }
            hosts.insert(full);
        }
        hosts.extend(identity.addresses.iter().map(|a| bracketed(a)));
        hosts.extend(extra_hosts.iter().map(|host| normalise_host(host)));

        Self {
            token: token.map(Arc::new),
            hosts: Arc::new(hosts),
            origins: Arc::new(origins.iter().map(|o| normalise_origin(o)).collect()),
            limiter: Arc::new(Mutex::new(RateLimiter::default())),
            peers: Arc::new(peers),
        }
    }

    /// The hosts this server answers for, so that rmcp's own transport can be
    /// given the same list and the two cannot disagree.
    pub fn hosts(&self) -> Vec<&str> {
        self.hosts.iter().map(String::as_str).collect()
    }

    /// Ask every question, in order, and answer the first that says no.
    ///
    /// The order matters: a request from the wrong host is refused before its
    /// token is looked at, so a page probing for a valid token learns nothing
    /// from the timing of the refusal.
    pub fn admit(&self, headers: &HeaderMap, address: IpAddr, now: Instant) -> Result<(), Refusal> {
        let host = headers
            .get(axum::http::header::HOST)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !self.hosts.contains(&normalise_host(host)) {
            return Err(Refusal::UnknownHost);
        }

        if let Some(origin) = headers.get(axum::http::header::ORIGIN) {
            let origin = normalise_origin(origin.to_str().unwrap_or_default());
            if !self.origins.contains(&origin) {
                return Err(Refusal::ForbiddenOrigin);
            }
        }

        if !self
            .limiter
            .lock()
            .map(|mut limiter| limiter.allow(address, now))
            .unwrap_or(true)
        {
            return Err(Refusal::RateLimited);
        }

        let Some(expected) = &self.token else {
            return Ok(());
        };
        let given = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| {
                value
                    .strip_prefix("Bearer ")
                    .or_else(|| value.strip_prefix("bearer "))
            })
            .unwrap_or_default();
        if same_secret(given.as_bytes(), expected.expose().as_bytes()) {
            Ok(())
        } else {
            Err(Refusal::BadToken)
        }
    }

    /// Who a request is from, as far as this server can tell.
    pub fn caller(&self, address: IpAddr) -> Caller {
        Caller {
            address,
            name: self.peers.get(&address).cloned(),
        }
    }
}

/// A host header, as the allow-list holds it: lowercased, without a port.
///
/// An IPv6 literal keeps its brackets, because that is what separates its
/// colons from the port's.
fn normalise_host(host: &str) -> String {
    let host = host.trim().to_ascii_lowercase();
    if let Some(rest) = host.strip_prefix('[') {
        let closed = rest.split_once(']').map(|(inside, _)| inside);
        return closed.map_or(host.clone(), |inside| format!("[{inside}]"));
    }
    host.split_once(':')
        .map_or(host.clone(), |(name, _)| name.to_owned())
}

/// An origin as RFC 6454 compares them: scheme, host and port, with the
/// default port for the scheme left off.
///
/// So an operator who listed `https://app.example` has also listed
/// `https://App.Example:443`, which is the same origin written differently and
/// is what a browser may actually send. `null` is a browser origin too — a
/// sandboxed frame's — and is left as it is so that listing it is possible and
/// deliberate.
fn normalise_origin(origin: &str) -> String {
    let origin = origin.trim();
    let Some((scheme, rest)) = origin.split_once("://") else {
        return origin.to_ascii_lowercase();
    };
    let scheme = scheme.to_ascii_lowercase();
    let authority = normalise_host(rest.split('/').next().unwrap_or_default());
    let port = rest
        .split('/')
        .next()
        .and_then(|a| a.rsplit_once(':'))
        .filter(|(before, _)| !before.ends_with(':') && !before.is_empty())
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .filter(|port| !matches!((scheme.as_str(), port), ("http", 80) | ("https", 443)));
    match port {
        Some(port) => format!("{scheme}://{authority}:{port}"),
        None => format!("{scheme}://{authority}"),
    }
}

/// An address as a `Host` header spells it: IPv6 in brackets, IPv4 bare.
fn bracketed(address: &str) -> String {
    if address.contains(':') {
        format!("[{}]", address.to_ascii_lowercase())
    } else {
        address.to_ascii_lowercase()
    }
}

/// Compare two secrets without letting the time taken say how much matched.
///
/// Length is folded in rather than checked first, so that a wrong-length token
/// takes the same path as a wrong one of the right length.
fn same_secret(given: &[u8], expected: &[u8]) -> bool {
    let mut difference = (given.len() ^ expected.len()) as u32;
    let longest = given.len().max(expected.len());
    for i in 0..longest {
        let a = given.get(i).copied().unwrap_or(0);
        let b = expected.get(i).copied().unwrap_or(0);
        difference |= u32::from(a ^ b);
    }
    difference == 0
}

/// One bucket per address, refilled by the passage of time.
#[derive(Debug, Default)]
struct RateLimiter {
    seen: HashMap<IpAddr, Bucket>,
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    /// How much of the burst is left.
    left: f64,
    /// When `left` was last brought up to date.
    at: Instant,
}

impl RateLimiter {
    /// Whether this address may make one more request now.
    fn allow(&mut self, address: IpAddr, now: Instant) -> bool {
        let rate = f64::from(RATE_BURST) / RATE_WINDOW.as_secs_f64();
        // An address whose bucket has had time to refill completely is
        // indistinguishable from one never seen, so it is forgotten rather
        // than kept: a server up for a year should not hold one entry per
        // address that ever reached it. The sweep is one pass over the
        // addresses currently spending, which is the same order as the work
        // the request itself is about to do.
        self.seen.retain(|_, bucket| {
            let refill = now.saturating_duration_since(bucket.at).as_secs_f64() * rate;
            refill < f64::from(RATE_BURST) - bucket.left
        });

        let bucket = self.seen.entry(address).or_insert(Bucket {
            left: f64::from(RATE_BURST),
            at: now,
        });
        let refill = now.saturating_duration_since(bucket.at).as_secs_f64() * rate;
        bucket.left = (bucket.left + refill).min(f64::from(RATE_BURST));
        bucket.at = now;
        if bucket.left < 1.0 {
            return false;
        }
        bucket.left -= 1.0;
        true
    }
}

/// The middleware every request but the health check passes through.
async fn admission(
    State(guard): State<Guard>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let address = peer.ip();
    if let Err(refusal) = guard.admit(request.headers(), address, Instant::now()) {
        tracing::warn!(
            caller = guard.caller(address).describe(),
            refusal = ?refusal,
            "refused an HTTP request"
        );
        return refusal.into_response();
    }
    // Where identity-derived authorisation goes when there is any: the caller
    // is resolved once, here, and everything downstream reads it from the
    // request rather than resolving it again.
    let caller = guard.caller(address);
    tracing::info!(caller = caller.describe(), path = %request.uri().path(), "http request");
    let mut request = request;
    request.extensions_mut().insert(caller);
    next.run(request).await
}

/// Build the router: the health check, the transport, and the checks in front.
pub fn router<S>(guard: Guard, mcp: S) -> axum::Router
where
    S: tower::Service<Request<Body>, Response = Response, Error = std::convert::Infallible>
        + Clone
        + Send
        + Sync
        + 'static,
    S::Future: Send + 'static,
{
    axum::Router::new()
        .route_service(MCP_PATH, mcp)
        .layer(axum::middleware::from_fn_with_state(
            guard.clone(),
            admission,
        ))
        // Outside the middleware, deliberately: a health check that needed a
        // token would not answer the question it exists to answer.
        .route(HEALTH_PATH, get(health))
        .with_state(guard)
}

/// Build the transport, the checks, and the listener, and serve until stopped.
///
/// rmcp's own transport validates `Host` and caps the body too, and is handed
/// the same allow-list and the same cap so the two cannot disagree. What it
/// does not do is this ticket's origin rule — its empty origin list means "do
/// not check" where the ticket means "refuse every browser" — nor a token, a
/// rate limit or an open health endpoint, which is why the checks in front of
/// it exist rather than being left to it (Q90).
pub async fn serve(
    settings: &crate::config::HttpConfig,
    guard: Guard,
    server: crate::server::TailscaleMcpServer,
) -> std::io::Result<()> {
    use rmcp::transport::streamable_http_server::{
        StreamableHttpService, session::local::LocalSessionManager,
    };

    let transport = StreamableHttpService::new(
        move || Ok(server.clone()),
        Arc::new(LocalSessionManager::default()),
        rmcp::transport::streamable_http_server::StreamableHttpServerConfig::default()
            .with_allowed_hosts(guard.hosts())
            .with_max_request_body_bytes(MAX_BODY_BYTES)
            .with_legacy_session_mode(settings.stateful),
    );
    // rmcp answers with a boxed body; axum's router routes `Body`. One map,
    // here, rather than a body type spelled through every signature below.
    let transport = tower::util::ServiceExt::<Request<Body>>::map_response(transport, |response| {
        axum::http::Response::map(response, Body::new)
    });

    let listener = tokio::net::TcpListener::bind(settings.bind).await?;
    tracing::info!(
        address = %settings.bind,
        authenticated = guard.token.is_some(),
        sessions = settings.stateful,
        "serving MCP over HTTP"
    );
    axum::serve(
        listener,
        router(guard, transport).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
}

async fn health() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        // The name, so that whatever polls this knows what answered, and not
        // the version: this is the one route that answers a caller holding no
        // credential, and the release it is looking at is not its business.
        concat!(
            "{\"status\":\"ok\",\"server\":\"",
            env!("CARGO_PKG_NAME"),
            "\"}\n"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> SelfIdentity {
        SelfIdentity {
            node_id: Some("n1111111CNTRL".to_owned()),
            numeric_id: None,
            addresses: vec!["100.64.0.1".to_owned(), "fd7a:115c:a1e0::1".to_owned()],
            dns_name: Some("workstation.example-tailnet.ts.net.".to_owned()),
        }
    }

    fn guard(token: Option<&str>, origins: &[&str]) -> Guard {
        Guard::new(
            token.map(tailscale_rest::Secret::new),
            &[],
            &origins.iter().map(|o| (*o).to_owned()).collect::<Vec<_>>(),
            &identity(),
            HashMap::new(),
        )
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).expect("a header name"),
                value.parse().expect("a header value"),
            );
        }
        headers
    }

    fn here() -> IpAddr {
        IpAddr::from([127, 0, 0, 1])
    }

    #[test]
    fn this_nodes_own_names_are_allowed_without_configuration() {
        let guard = guard(None, &[]);
        for host in [
            "localhost",
            "127.0.0.1:8449",
            "workstation",
            "workstation.example-tailnet.ts.net",
            "WORKSTATION.EXAMPLE-TAILNET.TS.NET:8449",
            "100.64.0.1:8449",
            "[fd7a:115c:a1e0::1]:8449",
        ] {
            assert_eq!(
                guard.admit(&headers(&[("host", host)]), here(), Instant::now()),
                Ok(()),
                "`{host}` is one of this node's own names"
            );
        }
    }

    #[test]
    fn a_host_this_server_does_not_answer_for_is_refused_before_anything_else() {
        // DNS rebinding: the page resolved its own name to this address, so
        // the socket is reached and the `Host` is the only thing that differs.
        let guard = guard(Some("s3cret-token-value"), &[]);
        assert_eq!(
            guard.admit(
                &headers(&[
                    ("host", "evil.example"),
                    ("authorization", "Bearer s3cret-token-value")
                ]),
                here(),
                Instant::now()
            ),
            Err(Refusal::UnknownHost),
            "and refused for the host, not for the token, which was right"
        );
    }

    #[test]
    fn a_browser_origin_is_refused_unless_it_was_listed() {
        let closed = guard(None, &[]);
        assert_eq!(
            closed.admit(
                &headers(&[("host", "localhost"), ("origin", "https://app.example")]),
                here(),
                Instant::now()
            ),
            Err(Refusal::ForbiddenOrigin)
        );

        let opened = guard(None, &["https://app.example"]);
        assert_eq!(
            opened.admit(
                &headers(&[("host", "localhost"), ("origin", "https://app.example")]),
                here(),
                Instant::now()
            ),
            Ok(())
        );
        // Listing one origin does not list another.
        assert_eq!(
            opened.admit(
                &headers(&[("host", "localhost"), ("origin", "https://other.example")]),
                here(),
                Instant::now()
            ),
            Err(Refusal::ForbiddenOrigin)
        );
    }

    #[test]
    fn the_token_has_to_match_and_a_missing_one_is_the_same_answer_as_a_wrong_one() {
        let guard = guard(Some("s3cret-token-value"), &[]);
        let host = ("host", "localhost");

        assert_eq!(
            guard.admit(
                &headers(&[host, ("authorization", "Bearer s3cret-token-value")]),
                here(),
                Instant::now()
            ),
            Ok(())
        );
        assert_eq!(
            guard.admit(&headers(&[host]), here(), Instant::now()),
            Err(Refusal::BadToken)
        );
        assert_eq!(
            guard.admit(
                &headers(&[host, ("authorization", "Bearer wrong")]),
                here(),
                Instant::now()
            ),
            Err(Refusal::BadToken)
        );
        // A prefix of the real token is not the real token.
        assert_eq!(
            guard.admit(
                &headers(&[host, ("authorization", "Bearer s3cret-token-valu")]),
                here(),
                Instant::now()
            ),
            Err(Refusal::BadToken)
        );
    }

    #[test]
    fn comparing_a_secret_folds_the_length_in_rather_than_checking_it_first() {
        assert!(same_secret(b"abc", b"abc"));
        assert!(!same_secret(b"abc", b"abd"));
        assert!(!same_secret(b"ab", b"abc"));
        assert!(!same_secret(b"abcd", b"abc"));
        assert!(same_secret(b"", b""));
        // A zero byte at the end of the shorter one is not a match: the length
        // difference is in the accumulator whatever the bytes say.
        assert!(!same_secret(b"abc", b"abc\0"));
    }

    #[test]
    fn the_rate_limit_triggers_and_then_recovers() {
        let mut limiter = RateLimiter::default();
        let start = Instant::now();
        for i in 0..RATE_BURST {
            assert!(
                limiter.allow(here(), start),
                "request {i} is inside the burst"
            );
        }
        assert!(
            !limiter.allow(here(), start),
            "and one more is not, at the same instant"
        );

        // Enough time for one token to come back.
        let later = start + RATE_WINDOW / RATE_BURST + Duration::from_millis(1);
        assert!(limiter.allow(here(), later), "a bucket refills with time");
        assert!(!limiter.allow(here(), later), "one at a time, though");

        // A different address has its own bucket.
        assert!(limiter.allow(IpAddr::from([127, 0, 0, 2]), start));
    }

    #[test]
    fn an_address_that_has_gone_quiet_is_forgotten() {
        let mut limiter = RateLimiter::default();
        let start = Instant::now();
        assert!(limiter.allow(here(), start));
        assert_eq!(limiter.seen.len(), 1, "while it is still spending");

        // A different address, long enough later that the first one's bucket
        // has refilled: the first is forgotten rather than kept for ever.
        assert!(limiter.allow(IpAddr::from([127, 0, 0, 2]), start + RATE_WINDOW * 2));
        assert_eq!(
            limiter.seen.keys().collect::<Vec<_>>(),
            vec![&IpAddr::from([127, 0, 0, 2])],
            "a server up for a year should not hold one entry per address that ever reached it"
        );
    }

    #[test]
    fn neither_the_guard_nor_the_settings_print_the_token() {
        // The one rule `Secret` exists for: `Config` derives `Debug`, and a
        // derived `Debug` on a `String` is how a token reaches a log.
        let guard = guard(Some("s3cret-token-value"), &[]);
        assert!(
            !format!("{guard:?}").contains("s3cret-token-value"),
            "the guard printed its token: {guard:?}"
        );
    }

    #[test]
    fn an_origin_is_compared_as_an_origin_and_not_as_a_string() {
        // What a browser sends and what an operator typed are the same origin
        // written two ways, and RFC 6454 says so.
        assert_eq!(
            normalise_origin("https://App.Example"),
            "https://app.example"
        );
        assert_eq!(
            normalise_origin("https://app.example:443"),
            "https://app.example",
            "the default port for the scheme is not part of the origin"
        );
        assert_eq!(normalise_origin("http://localhost:80"), "http://localhost");
        assert_eq!(
            normalise_origin("http://localhost:3000/some/page"),
            "http://localhost:3000",
            "a path is not part of an origin either"
        );
        assert_eq!(normalise_origin("null"), "null");

        let guard = guard(None, &["https://app.example"]);
        assert_eq!(
            guard.admit(
                &headers(&[("host", "localhost"), ("origin", "https://App.Example:443")]),
                here(),
                Instant::now()
            ),
            Ok(()),
            "listing an origin lists it however a browser spells it"
        );
    }

    #[test]
    fn a_host_header_is_matched_without_its_port_or_its_case() {
        assert_eq!(normalise_host("LocalHost:8449"), "localhost");
        assert_eq!(normalise_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(normalise_host("[::1]:8449"), "[::1]");
        assert_eq!(normalise_host("[FD7A:115C:A1E0::1]"), "[fd7a:115c:a1e0::1]");
        assert_eq!(
            normalise_host("  example-tailnet.ts.net  "),
            "example-tailnet.ts.net"
        );
    }
}
