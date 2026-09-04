//! The Streamable HTTP transport's checks, as a request actually meets them.
//!
//! The unit tests in `http.rs` ask the [`Guard`] its questions directly. These
//! drive the assembled router, which is where the answers turn into statuses
//! and where the health endpoint's exemption either holds or does not: it is
//! outside the middleware by one line, and a test that only asked the guard
//! would never notice that line going missing.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use axum::body::Body;
use axum::http::{Request, Response, StatusCode};
use tailscale_mcp::config::{Cli, Config};
use tailscale_mcp::context::SelfIdentity;
use tailscale_mcp::http::{self, Guard};
use tower::ServiceExt as _;

/// This node, as status would report it.
fn identity() -> SelfIdentity {
    SelfIdentity {
        node_id: Some("n1111111CNTRL".to_owned()),
        numeric_id: None,
        addresses: vec!["100.64.0.1".to_owned()],
        dns_name: Some("workstation.example-tailnet.ts.net.".to_owned()),
    }
}

/// A router with a stand-in for the MCP transport, so that a request that gets
/// through is visibly a request that got through.
fn router(token: Option<&str>, hosts: &[&str], origins: &[&str]) -> axum::Router {
    let guard = Guard::new(
        token.map(tailscale_rest::Secret::new),
        &hosts.iter().map(|h| (*h).to_owned()).collect::<Vec<_>>(),
        &origins.iter().map(|o| (*o).to_owned()).collect::<Vec<_>>(),
        &identity(),
        HashMap::from([(
            IpAddr::from([100, 64, 0, 2]),
            "laptop.example-tailnet.ts.net".to_owned(),
        )]),
    );
    http::router(
        guard,
        tower::service_fn(|request: Request<Body>| async move {
            // What the transport is handed about its caller. The ticket asks
            // that identity-derived authorisation be addable "without changing
            // the transport", which is only true if the caller is already
            // there to authorise on.
            let caller = request
                .extensions()
                .get::<http::Caller>()
                .map_or_else(|| "nobody".to_owned(), http::Caller::describe);
            Ok::<_, std::convert::Infallible>(Response::new(Body::from(caller)))
        }),
    )
}

/// The body a request came back with.
async fn body_of(
    router: &axum::Router,
    path: &str,
    headers: &[(&str, &str)],
    from: IpAddr,
) -> String {
    let response = send(router, path, headers, from).await;
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("a small body");
    String::from_utf8(bytes.to_vec()).expect("text")
}

/// Send one request from `127.0.0.1` with these headers.
async fn ask(router: &axum::Router, path: &str, headers: &[(&str, &str)]) -> StatusCode {
    ask_from(router, path, headers, IpAddr::from([127, 0, 0, 1])).await
}

async fn ask_from(
    router: &axum::Router,
    path: &str,
    headers: &[(&str, &str)],
    from: IpAddr,
) -> StatusCode {
    send(router, path, headers, from).await.status()
}

async fn send(
    router: &axum::Router,
    path: &str,
    headers: &[(&str, &str)],
    from: IpAddr,
) -> Response<Body> {
    let mut request = Request::builder().uri(path);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut request = request.body(Body::empty()).expect("a request");
    request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(SocketAddr::new(from, 51000)));
    router
        .clone()
        .oneshot(request)
        .await
        .expect("the router answers")
}

#[tokio::test]
async fn a_request_with_the_right_token_reaches_the_transport_and_others_do_not() {
    let router = router(Some("s3cret-token-value"), &[], &[]);
    let host = ("host", "localhost:8449");

    assert_eq!(
        ask(
            &router,
            http::MCP_PATH,
            &[host, ("authorization", "Bearer s3cret-token-value")]
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        ask(&router, http::MCP_PATH, &[host]).await,
        StatusCode::UNAUTHORIZED,
        "a missing token"
    );
    assert_eq!(
        ask(
            &router,
            http::MCP_PATH,
            &[host, ("authorization", "Bearer nearly-the-right-token")]
        )
        .await,
        StatusCode::UNAUTHORIZED,
        "and a wrong one, told apart from a missing one by nothing the caller can see"
    );
}

#[tokio::test]
async fn this_nodes_own_tailnet_name_is_answered_for_and_a_stranger_is_not() {
    let router = router(None, &[], &[]);

    for host in [
        "localhost",
        "127.0.0.1:8449",
        // Neither of these was configured: they came from status.
        "workstation",
        "workstation.example-tailnet.ts.net:8449",
        "100.64.0.1:8449",
    ] {
        assert_eq!(
            ask(&router, http::MCP_PATH, &[("host", host)]).await,
            StatusCode::OK,
            "`{host}` names this node"
        );
    }

    // DNS rebinding: the page pointed its own name at this address, so the
    // socket is reached and only the `Host` gives it away.
    assert_eq!(
        ask(&router, http::MCP_PATH, &[("host", "evil.example")]).await,
        StatusCode::FORBIDDEN
    );

    // And an operator can add one.
    let extended = router_with_host("proxy.internal");
    assert_eq!(
        ask(&extended, http::MCP_PATH, &[("host", "proxy.internal")]).await,
        StatusCode::OK
    );
}

fn router_with_host(host: &str) -> axum::Router {
    router(None, &[host], &[])
}

#[tokio::test]
async fn a_browser_origin_is_refused_until_it_is_listed() {
    let closed = router(None, &[], &[]);
    assert_eq!(
        ask(
            &closed,
            http::MCP_PATH,
            &[("host", "localhost"), ("origin", "https://app.example")]
        )
        .await,
        StatusCode::FORBIDDEN,
        "a request carrying an Origin came from a page, and no page was invited"
    );

    let opened = router(None, &[], &["https://app.example"]);
    assert_eq!(
        ask(
            &opened,
            http::MCP_PATH,
            &[("host", "localhost"), ("origin", "https://app.example")]
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        ask(
            &opened,
            http::MCP_PATH,
            &[("host", "localhost"), ("origin", "https://other.example")]
        )
        .await,
        StatusCode::FORBIDDEN,
        "inviting one page does not invite the rest"
    );
}

#[tokio::test]
async fn the_health_endpoint_answers_with_no_token_and_no_host_of_ours() {
    let router = router(Some("s3cret-token-value"), &[], &[]);
    assert_eq!(
        ask(&router, http::HEALTH_PATH, &[("host", "evil.example")]).await,
        StatusCode::OK,
        "a health check that needed a credential would not answer the question it is for"
    );
    // The same request one path over is refused, which is what says the
    // exemption is the health endpoint's and not everyone's.
    assert_eq!(
        ask(&router, http::MCP_PATH, &[("host", "evil.example")]).await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn the_rate_limit_triggers_and_then_lets_the_address_back_in() {
    let router = router(None, &[], &[]);
    let caller = IpAddr::from([100, 64, 0, 2]);
    let host = [("host", "localhost")];

    for i in 0..http::RATE_BURST {
        assert_eq!(
            ask_from(&router, http::MCP_PATH, &host, caller).await,
            StatusCode::OK,
            "request {i} is inside the burst"
        );
    }
    assert_eq!(
        ask_from(&router, http::MCP_PATH, &host, caller).await,
        StatusCode::TOO_MANY_REQUESTS
    );

    // Another address is unaffected: the limit is per caller, so one noisy
    // client cannot lock everyone else out.
    assert_eq!(
        ask_from(
            &router,
            http::MCP_PATH,
            &host,
            IpAddr::from([100, 64, 0, 3])
        )
        .await,
        StatusCode::OK
    );

    // And the bucket refills: waiting is enough, and no restart is needed.
    tokio::time::sleep(http::RATE_WINDOW / http::RATE_BURST + std::time::Duration::from_millis(50))
        .await;
    assert_eq!(
        ask_from(&router, http::MCP_PATH, &host, caller).await,
        StatusCode::OK,
        "the limit recovers on its own"
    );
}

/// Binding somewhere other than loopback without a token is refused, and the
/// flag is the only way past it.
#[test]
fn publishing_an_unauthenticated_control_plane_has_to_be_said_out_loud() {
    let resolve = |args: &[&str]| {
        let cli = <Cli as clap::Parser>::try_parse_from(args).expect("the arguments parse");
        Config::resolve_with(cli, |_| None)
    };
    let resolve_with_token = |args: &[&str]| {
        let cli = <Cli as clap::Parser>::try_parse_from(args).expect("the arguments parse");
        Config::resolve_with(cli, |key| {
            (key == tailscale_mcp::config::HTTP_TOKEN_ENV).then(|| "s3cret-token-value".to_owned())
        })
    };

    let refused = resolve(&["tailscale-mcp", "--http", "0.0.0.0:8449"])
        .expect_err("a bare non-loopback bind is refused");
    assert!(
        refused.to_string().contains("--http-no-auth"),
        "and the refusal says what to do about it: {refused}"
    );

    assert!(
        resolve(&["tailscale-mcp", "--http", "0.0.0.0:8449", "--http-no-auth"]).is_ok(),
        "the flag is the way past, and is a flag rather than an omission"
    );
    assert!(
        resolve_with_token(&["tailscale-mcp", "--http", "0.0.0.0:8449"]).is_ok(),
        "as is a token, which comes from the environment because the command \
         line is not a place to put one"
    );

    // Loopback needs neither: the operating system already decided who can
    // reach the socket.
    let loopback = resolve(&["tailscale-mcp", "--http"]).expect("a bare --http is loopback");
    let http = loopback.http.expect("the transport was asked for");
    assert_eq!(http.bind.to_string(), http::DEFAULT_BIND);
    assert!(http.token.is_none());
    assert!(http.stateful, "sessions unless the operator says otherwise");
}

/// The caller reaches the transport already resolved.
#[tokio::test]
async fn a_request_carries_the_caller_the_tailnet_knows_it_as() {
    let router = router(None, &[], &[]);
    let host = [("host", "localhost")];

    assert_eq!(
        body_of(
            &router,
            http::MCP_PATH,
            &host,
            IpAddr::from([100, 64, 0, 2])
        )
        .await,
        "laptop.example-tailnet.ts.net (100.64.0.2)",
        "a peer this node can name is named"
    );
    assert_eq!(
        body_of(&router, http::MCP_PATH, &host, IpAddr::from([127, 0, 0, 1])).await,
        "127.0.0.1",
        "and one it cannot stays an address rather than becoming a guess"
    );
}

/// The stateless switch reaches the transport rather than stopping at the flag.
#[test]
fn the_stateless_mode_is_the_operators_to_choose() {
    let resolve = |args: &[&str]| {
        let cli = <Cli as clap::Parser>::try_parse_from(args).expect("the arguments parse");
        Config::resolve_with(cli, |_| None)
            .expect("the configuration resolves")
            .http
            .expect("the transport was asked for")
    };

    assert!(resolve(&["tailscale-mcp", "--http"]).stateful);
    assert!(!resolve(&["tailscale-mcp", "--http", "--http-stateless"]).stateful);

    // And through the variable, which is what a container sets.
    let cli = <Cli as clap::Parser>::try_parse_from(["tailscale-mcp", "--http"])
        .expect("the arguments parse");
    let from_environment = Config::resolve_with(cli, |key| {
        (key == tailscale_mcp::config::HTTP_STATELESS_ENV).then(|| "true".to_owned())
    })
    .expect("the configuration resolves")
    .http
    .expect("the transport was asked for");
    assert!(!from_environment.stateful);
}

/// The token does not print itself, wherever it has got to.
#[test]
fn the_configuration_does_not_print_the_token() {
    let cli = <Cli as clap::Parser>::try_parse_from(["tailscale-mcp", "--http"])
        .expect("the arguments parse");
    let config = Config::resolve_with(cli, |key| {
        (key == tailscale_mcp::config::HTTP_TOKEN_ENV).then(|| "s3cret-token-value".to_owned())
    })
    .expect("the configuration resolves");

    assert!(
        !format!("{config:?}").contains("s3cret-token-value"),
        "a derived Debug on a String field is how tokens end up in logs: {config:?}"
    );
}
