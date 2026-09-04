//! A control plane that answers on a socket, for tests.
//!
//! Faking at the client's own interface would skip the parts most likely to be
//! wrong — the request line, the authorization header, query construction,
//! status handling, retry — so this speaks HTTP/1.1 on a real loopback socket
//! and records exactly what arrived.
//!
//! Deliberately not a general-purpose server: it understands the small part of
//! HTTP that a JSON API client uses, and nothing else.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

/// A request as it arrived on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub method: String,
    /// The path with no query string.
    pub path: String,
    /// Query parameters, parsed and sorted.
    pub query: BTreeMap<String, String>,
    /// Header names lowercased, since HTTP header names are case-insensitive.
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

impl Recorded {
    /// The body as JSON, for asserting on what was sent.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }

    /// The `Authorization` header, if there was one.
    pub fn authorization(&self) -> Option<&str> {
        self.headers.get("authorization").map(String::as_str)
    }

    /// One header by name, which the caller gives lowercased.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
}

/// What the fake sends back.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    /// How long to take over it. A client's concurrency limit cannot be
    /// observed against a server that answers instantly.
    pub delay: std::time::Duration,
    /// Send it without a `Content-Length`, the way a streamed answer arrives.
    pub chunked: bool,
}

impl Response {
    /// A JSON body with a 200.
    pub fn json(body: impl serde::Serialize) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".to_owned(), "application/json".to_owned())],
            body: serde_json::to_string(&body).unwrap_or_else(|_| "null".to_owned()),
            delay: std::time::Duration::ZERO,
            chunked: false,
        }
    }

    /// A JSON body with a chosen status.
    pub fn status(status: u16, body: impl serde::Serialize) -> Self {
        Self {
            status,
            ..Self::json(body)
        }
    }

    /// A body that is not JSON, under a content type of its own.
    ///
    /// The policy file is why: it is HuJSON, and a fake that could only send
    /// JSON could not stand in for the one endpoint whose document is not.
    pub fn text(content_type: &str, body: impl Into<String>) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".to_owned(), content_type.to_owned())],
            body: body.into(),
            delay: std::time::Duration::ZERO,
            chunked: false,
        }
    }

    /// An empty success, which is what the API returns from most deletions.
    pub fn empty() -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body: String::new(),
            delay: std::time::Duration::ZERO,
            chunked: false,
        }
    }

    /// Add a header, for `Retry-After` and the like.
    #[must_use]
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers
            .push((name.to_ascii_lowercase(), value.to_owned()));
        self
    }

    /// Take this long over it.
    #[must_use]
    pub fn slow(mut self, delay: std::time::Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Send it chunked, so the client is not told the length in advance.
    #[must_use]
    pub fn chunked(mut self) -> Self {
        self.chunked = true;
        self
    }
}

/// One thing the fake will answer, and how many times.
#[derive(Debug, Clone)]
struct Rule {
    method: Option<String>,
    path: Option<String>,
    response: Response,
    remaining: Option<usize>,
}

impl Rule {
    fn matches(&self, method: &str, path: &str) -> bool {
        self.remaining != Some(0)
            && self.method.as_ref().is_none_or(|m| m == method)
            && self.path.as_ref().is_none_or(|p| p == path)
    }
}

#[derive(Debug, Default)]
struct State {
    rules: Vec<Rule>,
    recorded: Vec<Recorded>,
    /// Requests currently being answered, and the most there have ever been
    /// at once. The high-water mark is the only way to see a client's
    /// concurrency limit from this side.
    serving: usize,
    peak_serving: usize,
}

/// A running fake control plane. Stops when it is dropped.
#[derive(Debug)]
pub struct FakeControlPlane {
    base_url: String,
    state: Arc<Mutex<State>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for FakeControlPlane {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl FakeControlPlane {
    /// Bind a loopback socket and start answering. Fails only if the machine
    /// has no loopback interface, in which case no test can run anyway.
    pub async fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let base_url = format!("http://{}", listener.local_addr()?);
        let state = Arc::new(Mutex::new(State::default()));

        let serving = Arc::clone(&state);
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let state = Arc::clone(&serving);
                tokio::spawn(async move {
                    // A connection that goes wrong is the client's problem to
                    // notice; there is nowhere useful to report it from here.
                    let _ = serve(stream, state).await;
                });
            }
        });

        Ok(Self {
            base_url,
            state,
            task,
        })
    }

    /// Where the client should point.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Answer any request that has not already been answered by an earlier
    /// rule. Rules are tried in the order they were added.
    #[must_use]
    pub fn always(self, response: Response) -> Self {
        self.push(None, None, response, None)
    }

    /// Answer one method and path, as many times as it is asked.
    #[must_use]
    pub fn on(self, method: &str, path: &str, response: Response) -> Self {
        self.push(
            Some(method.to_ascii_uppercase()),
            Some(path.to_owned()),
            response,
            None,
        )
    }

    /// Answer one method and path exactly once, so that a sequence of
    /// different answers to the same request can be set up.
    #[must_use]
    pub fn once(self, method: &str, path: &str, response: Response) -> Self {
        self.push(
            Some(method.to_ascii_uppercase()),
            Some(path.to_owned()),
            response,
            Some(1),
        )
    }

    fn push(
        self,
        method: Option<String>,
        path: Option<String>,
        response: Response,
        remaining: Option<usize>,
    ) -> Self {
        if let Ok(mut state) = self.state.lock() {
            state.rules.push(Rule {
                method,
                path,
                response,
                remaining,
            });
        }
        self
    }

    /// Everything that arrived, in order.
    pub fn recorded(&self) -> Vec<Recorded> {
        self.state
            .lock()
            .map(|s| s.recorded.clone())
            .unwrap_or_default()
    }

    /// How many requests arrived.
    pub fn request_count(&self) -> usize {
        self.recorded().len()
    }

    /// The most requests that were ever being answered at the same moment.
    ///
    /// Only meaningful against responses that take some time; see
    /// [`Response::slow`].
    pub fn peak_concurrency(&self) -> usize {
        self.state
            .lock()
            .map(|s| s.peak_serving)
            .unwrap_or_default()
    }

    /// The single request that arrived, when a test expects exactly one.
    ///
    /// # Panics
    /// If a different number of requests arrived.
    pub fn only_request(&self) -> Recorded {
        let recorded = self.recorded();
        assert_eq!(
            recorded.len(),
            1,
            "expected exactly one request, got {recorded:#?}"
        );
        recorded
            .into_iter()
            .next()
            .unwrap_or_else(|| unreachable!())
    }
}

/// Read requests off one connection until the client goes away.
async fn serve(mut stream: TcpStream, state: Arc<Mutex<State>>) -> std::io::Result<()> {
    let mut buffer = Vec::new();
    loop {
        let Some(request) = read_request(&mut stream, &mut buffer).await? else {
            return Ok(());
        };

        let response = {
            let Ok(mut state) = state.lock() else {
                return Ok(());
            };
            let found = state
                .rules
                .iter_mut()
                .find(|r| r.matches(&request.method, &request.path))
                .map(|rule| {
                    if let Some(remaining) = rule.remaining.as_mut() {
                        *remaining -= 1;
                    }
                    rule.response.clone()
                });
            state.recorded.push(request);
            state.serving += 1;
            state.peak_serving = state.peak_serving.max(state.serving);
            found.unwrap_or_else(|| {
                Response::status(
                    501,
                    serde_json::json!({ "message": "the fake control plane has no rule for this" }),
                )
            })
        };

        // Outside the lock: the point of the delay is that other connections
        // keep being served while this one waits.
        if !response.delay.is_zero() {
            tokio::time::sleep(response.delay).await;
        }
        let written = stream.write_all(&render(&response)).await;
        if let Ok(mut state) = state.lock() {
            state.serving -= 1;
        }
        written?;
        stream.flush().await?;
    }
}

/// Parse one request, returning `None` when the connection ended cleanly.
async fn read_request(
    stream: &mut TcpStream,
    buffer: &mut Vec<u8>,
) -> std::io::Result<Option<Recorded>> {
    // Headers first, which end at the blank line.
    let head_end = loop {
        if let Some(at) = find(buffer, b"\r\n\r\n") {
            break at + 4;
        }
        let mut chunk = [0_u8; 4096];
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(None);
        }
        buffer.extend_from_slice(&chunk[..read]);
    };

    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.lines();
    let Some(request_line) = lines.next() else {
        return Ok(None);
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default();

    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_owned(), parse_query(query)),
        None => (target.to_owned(), BTreeMap::new()),
    };

    let mut headers = BTreeMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }

    let length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    while buffer.len() < head_end + length {
        let mut chunk = [0_u8; 4096];
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8_lossy(&buffer[head_end..head_end + length]).into_owned();
    buffer.drain(..head_end + length);

    Ok(Some(Recorded {
        method,
        path,
        query,
        headers,
        body,
    }))
}

fn render(response: &Response) -> Vec<u8> {
    let framing = if response.chunked {
        "transfer-encoding: chunked\r\n".to_owned()
    } else {
        format!("content-length: {}\r\n", response.body.len())
    };
    let mut head = format!(
        "HTTP/1.1 {} {}\r\n{framing}",
        response.status,
        reason(response.status),
    );
    for (name, value) in &response.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let mut bytes = head.into_bytes();
    if response.chunked {
        // Two chunks and a terminator, so that a client reading incrementally
        // has to read more than once to reach the end.
        let body = response.body.as_bytes();
        let (first, second) = body.split_at(body.len() / 2);
        for chunk in [first, second] {
            if !chunk.is_empty() {
                bytes.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
                bytes.extend_from_slice(chunk);
                bytes.extend_from_slice(b"\r\n");
            }
        }
        bytes.extend_from_slice(b"0\r\n\r\n");
    } else {
        bytes.extend_from_slice(response.body.as_bytes());
    }
    bytes
}

/// Enough of the reason phrases to be readable in a packet dump. Clients do
/// not read them, so an unknown status gets a placeholder rather than a table.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

fn parse_query(query: &str) -> BTreeMap<String, String> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => (decode(k), decode(v)),
            None => (decode(pair), String::new()),
        })
        .collect()
}

/// Percent-decoding, enough for the values a query string carries.
fn decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fake exists to be talked to by a real HTTP client, so the tests use
    /// one: anything it gets wrong about the wire format shows up here rather
    /// than as a mystery in a client test.
    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .build()
            .expect("a client with no options")
    }

    #[tokio::test]
    async fn a_get_is_answered_and_recorded() {
        let fake = FakeControlPlane::start().await.expect("loopback").on(
            "GET",
            "/api/v2/tailnet/-/devices",
            Response::json(serde_json::json!({ "devices": [] })),
        );

        let response = client()
            .get(format!("{}/api/v2/tailnet/-/devices", fake.base_url()))
            .header("authorization", "Bearer tskey-api-example")
            .send()
            .await
            .expect("the fake answers");

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.expect("a JSON body");
        assert_eq!(body["devices"], serde_json::json!([]));

        let recorded = fake.only_request();
        assert_eq!(recorded.method, "GET");
        assert_eq!(recorded.path, "/api/v2/tailnet/-/devices");
        assert_eq!(recorded.authorization(), Some("Bearer tskey-api-example"));
    }

    #[tokio::test]
    async fn a_body_and_a_query_string_arrive_intact() {
        let fake = FakeControlPlane::start()
            .await
            .expect("loopback")
            .always(Response::empty());

        client()
            .post(format!(
                "{}/api/v2/device/n1/routes?all=true&q=a+b",
                fake.base_url()
            ))
            .json(&serde_json::json!({ "routes": ["10.0.0.0/8"] }))
            .send()
            .await
            .expect("the fake answers");

        let recorded = fake.only_request();
        assert_eq!(recorded.method, "POST");
        assert_eq!(recorded.path, "/api/v2/device/n1/routes");
        assert_eq!(recorded.query.get("all").map(String::as_str), Some("true"));
        assert_eq!(recorded.query.get("q").map(String::as_str), Some("a b"));
        assert_eq!(recorded.json()["routes"][0], "10.0.0.0/8");
    }

    #[tokio::test]
    async fn one_connection_carries_several_requests() {
        // reqwest keeps the connection alive, so a fake that mishandled that
        // would hang the second call rather than fail it.
        let fake = FakeControlPlane::start()
            .await
            .expect("loopback")
            .always(Response::json(serde_json::json!({ "ok": true })));
        let client = client();

        for _ in 0..3 {
            let response = client
                .get(format!("{}/api/v2/tailnet/-/devices", fake.base_url()))
                .send()
                .await
                .expect("the fake answers");
            assert_eq!(response.status(), 200);
        }
        assert_eq!(fake.request_count(), 3);
    }

    #[tokio::test]
    async fn a_sequence_of_answers_can_be_set_up() {
        let fake = FakeControlPlane::start()
            .await
            .expect("loopback")
            .once(
                "GET",
                "/api/v2/tailnet/-/devices",
                Response::status(429, serde_json::json!({ "message": "slow down" }))
                    .with_header("retry-after", "1"),
            )
            .on(
                "GET",
                "/api/v2/tailnet/-/devices",
                Response::json(serde_json::json!({ "devices": [] })),
            );
        let client = client();
        let url = format!("{}/api/v2/tailnet/-/devices", fake.base_url());

        let first = client.get(&url).send().await.expect("answered");
        assert_eq!(first.status(), 429);
        assert_eq!(
            first
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok()),
            Some("1")
        );

        let second = client.get(&url).send().await.expect("answered");
        assert_eq!(second.status(), 200);
    }

    #[tokio::test]
    async fn an_unmatched_request_is_loudly_wrong_rather_than_plausible() {
        let fake = FakeControlPlane::start().await.expect("loopback");
        let response = client()
            .get(format!("{}/api/v2/tailnet/-/keys", fake.base_url()))
            .send()
            .await
            .expect("answered");

        // Not a 404: a test that forgot to set up a rule should not look like
        // a test of what happens when something is missing.
        assert_eq!(response.status(), 501);
    }
}
