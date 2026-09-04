//! The tool-level error model, and the redaction every error passes through.
//!
//! Two rules shape this module.
//!
//! An operation that runs and fails is a *result*, not a protocol error: the
//! model asked a sensible question and deserves a structured answer it can act
//! on. Protocol errors are reserved for requests that were malformed before any
//! work began — an unknown tool, arguments that do not fit the schema.
//!
//! Every error path can carry a secret, because the thing that failed was
//! usually handed one. Redaction therefore lives here, on the type, rather than
//! at each call site where it would eventually be forgotten.

use std::borrow::Cow;
use std::fmt;

use serde::Serialize;

/// The fixed vocabulary of failures a tool can report.
///
/// Fixed is the operative word: a client can branch on these, so a new variant
/// is a compatibility question and not a detail. The text of each is stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The `tailscale` binary ran and exited non-zero.
    CliFailed,
    /// The control plane returned a status we do not model more precisely.
    ApiError,
    /// The operation did not finish inside its budget.
    Timeout,
    /// The tool exists but this server was not started with the tier or
    /// toolset that permits it.
    NotPermitted,
    /// The local node refuses the command because the caller is not its
    /// configured operator.
    NeedsOperator,
    /// The installed `tailscale` is older than the command requires.
    UnsupportedVersion,
    /// The backend a tool needs is absent: no binary on the path, no
    /// credential configured, or a daemon that is not answering.
    BackendUnavailable,
    /// Arguments parsed but do not describe a workable request.
    InvalidArgs,
    /// The command does not exist on this operating system.
    UnsupportedPlatform,
    /// The target of the operation does not exist.
    NotFound,
    /// The state changed underneath us: a stale ETag, or a resource that
    /// already exists.
    Conflict,
    /// The control plane asked us to slow down.
    RateLimited,
    /// The result would exceed the configured size cap.
    ResultTooLarge,
    /// The operation is one the caller must state intent for.
    ConfirmationRequired,
}

impl ErrorCode {
    /// Every code, used by the test that proves each one is reachable.
    pub const ALL: &'static [ErrorCode] = &[
        Self::CliFailed,
        Self::ApiError,
        Self::Timeout,
        Self::NotPermitted,
        Self::NeedsOperator,
        Self::UnsupportedVersion,
        Self::BackendUnavailable,
        Self::InvalidArgs,
        Self::UnsupportedPlatform,
        Self::NotFound,
        Self::Conflict,
        Self::RateLimited,
        Self::ResultTooLarge,
        Self::ConfirmationRequired,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CliFailed => "cli_failed",
            Self::ApiError => "api_error",
            Self::Timeout => "timeout",
            Self::NotPermitted => "not_permitted",
            Self::NeedsOperator => "needs_operator",
            Self::UnsupportedVersion => "unsupported_version",
            Self::BackendUnavailable => "backend_unavailable",
            Self::InvalidArgs => "invalid_args",
            Self::UnsupportedPlatform => "unsupported_platform",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::RateLimited => "rate_limited",
            Self::ResultTooLarge => "result_too_large",
            Self::ConfirmationRequired => "confirmation_required",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A failed tool call, as the client sees it.
///
/// Every string field has already been through [`redact`] by the time it is
/// here: the constructors do it, so a caller cannot forget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolError {
    pub code: ErrorCode,
    pub message: String,
    /// The process exit code, when a process is what failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// What the process wrote to its standard error, trimmed and redacted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    /// The HTTP status, when the control plane is what failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// What the caller can do about it. Present on every code where the fix is
    /// something the caller or operator controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl ToolError {
    /// The general constructor. Prefer the named ones below; this exists for
    /// the paths that compute their own code.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: redact(&message.into()).into_owned(),
            exit_code: None,
            stderr: None,
            status: None,
            hint: None,
        }
    }

    #[must_use]
    pub fn with_exit_code(mut self, exit_code: i32) -> Self {
        self.exit_code = Some(exit_code);
        self
    }

    #[must_use]
    pub fn with_stderr(mut self, stderr: impl AsRef<str>) -> Self {
        let trimmed = stderr.as_ref().trim();
        if !trimmed.is_empty() {
            self.stderr = Some(redact(trimmed).into_owned());
        }
        self
    }

    #[must_use]
    pub fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(redact(&hint.into()).into_owned());
        self
    }

    /// The `tailscale` binary exited non-zero.
    ///
    /// The code is optional because a process killed by a signal has none, and
    /// saying so is more use to the caller than inventing a number.
    pub fn cli_failed(argv0: &str, exit_code: Option<i32>, stderr: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::CliFailed,
            match exit_code {
                Some(code) => format!("`{argv0}` exited with status {code}"),
                None => format!("`{argv0}` was terminated before it exited"),
            },
        )
        .with_stderr(stderr);
        if let Some(code) = exit_code {
            err = err.with_exit_code(code);
        }
        err
    }

    /// The control plane returned a status we do not model more precisely.
    pub fn api_error(status: u16, body: &str) -> Self {
        let body = body.trim();
        let message = if body.is_empty() {
            format!("the control plane returned HTTP {status}")
        } else {
            format!("the control plane returned HTTP {status}: {body}")
        };
        Self::new(ErrorCode::ApiError, message).with_status(status)
    }

    /// A command that did not finish. `printed` is whatever it had said before
    /// it was stopped, which for a command that waits on someone else is
    /// usually the whole explanation.
    pub fn timeout(what: &str, seconds: u64, printed: &str) -> Self {
        let printed = printed.trim();
        let mut message = format!("{what} did not finish within {seconds}s");
        if !printed.is_empty() {
            message.push_str(", having said: ");
            message.push_str(printed);
        }
        Self::new(ErrorCode::Timeout, message).with_hint(if printed.is_empty() {
            "Raise the timeout, or narrow what the call asks for."
        } else {
            "The command was waiting on something. Act on what it printed, then call again."
        })
    }

    /// A tool was reached that this server is not permitted to run. In the
    /// normal case such tools are hidden rather than refused, so this fires
    /// when a client calls a name it did not get from the listing.
    pub fn not_permitted(tool: &str, needs: &str) -> Self {
        Self::new(
            ErrorCode::NotPermitted,
            format!("`{tool}` is not available on this server"),
        )
        .with_hint(format!("Start the server with {needs} to enable it."))
    }

    pub fn needs_operator(stderr: &str) -> Self {
        Self::new(
            ErrorCode::NeedsOperator,
            "the local node refused the command because this user is not its operator",
        )
        .with_stderr(stderr)
        .with_hint(
            "Run `tailscale set --operator=$USER` as an administrator, \
             or run the server as the operator user.",
        )
    }

    pub fn unsupported_version(tool: &str, needs: &str, found: &str) -> Self {
        Self::new(
            ErrorCode::UnsupportedVersion,
            format!("`{tool}` needs Tailscale {needs} or newer; this node runs {found}"),
        )
        .with_hint("Upgrade Tailscale on this node.")
    }

    pub fn backend_unavailable(what: &str, why: &str) -> Self {
        Self::new(
            ErrorCode::BackendUnavailable,
            format!("{what} is unavailable: {why}"),
        )
    }

    pub fn invalid_args(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgs, message)
    }

    pub fn unsupported_platform(tool: &str, platform: &str) -> Self {
        Self::new(
            ErrorCode::UnsupportedPlatform,
            format!("`{tool}` does not exist on {platform}"),
        )
        .with_hint("This command is available on other operating systems only.")
    }

    pub fn not_found(what: &str) -> Self {
        Self::new(ErrorCode::NotFound, format!("{what} was not found")).with_status(404)
    }

    /// A version this caller holds is no longer the current one. Carries 409
    /// the way `not_found` carries 404: a client that branches on the status
    /// should not have to know which of the two this server chose to name.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
            .with_status(409)
            .with_hint(
                "Re-read the resource to get its current version, then retry with that version.",
            )
    }

    pub fn rate_limited(retry_after: Option<u64>) -> Self {
        let err = Self::new(
            ErrorCode::RateLimited,
            "the control plane is rate-limiting this client",
        )
        .with_status(429);
        match retry_after {
            Some(secs) => err.with_hint(format!("Retry after {secs}s.")),
            None => err.with_hint("Retry after a short delay."),
        }
    }

    pub fn result_too_large(bytes: usize, cap: usize) -> Self {
        Self::new(
            ErrorCode::ResultTooLarge,
            format!("the result is {bytes} bytes, over the {cap} byte cap"),
        )
        .with_hint(TOO_LARGE_HINT)
    }

    pub fn confirmation_required(tool: &str, consequence: &str) -> Self {
        Self::new(
            ErrorCode::ConfirmationRequired,
            format!("`{tool}` {consequence}"),
        )
        .with_hint("Repeat the call with `confirm: true` if that is what you intend.")
    }

    /// The wire form: what a client receives as the structured content of a
    /// failed call. Falls back to a bare code if serialisation ever fails, so
    /// that a caller always gets something it can branch on.
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(
            |_| serde_json::json!({ "code": self.code.as_str(), "message": self.message }),
        )
    }
}

/// The two ways out of a result that will not fit, in the words a caller can
/// act on. Shared by the tool-result cap and the transport's, which are the
/// same cap seen from either end.
const TOO_LARGE_HINT: &str =
    "Narrow the request, or raise TAILSCALE_MCP_MAX_RESULT_BYTES on the server.";

/// A control-plane failure, in the vocabulary a client can branch on.
///
/// `tailscale_rest` deliberately names its variants for what happened rather
/// than for what a caller should be told, so that the crate can be used without
/// this server's error model. This is the other half of that arrangement, and
/// the one place the translation happens: every tailnet tool reaches the
/// control plane through `?`, so nothing has to remember to call it.
impl From<tailscale_rest::ApiError> for ToolError {
    fn from(error: tailscale_rest::ApiError) -> Self {
        use tailscale_rest::ApiError as Api;

        match &error {
            // The statuses the model has its own code for. Everything else
            // keeps the number, because a client that knows the control-plane
            // API can read a status this server has no opinion about.
            Api::Status {
                status, message, ..
            } if *status == 404 => Self::new(ErrorCode::NotFound, message.clone()).with_status(404),
            Api::Status {
                status, message, ..
            } if *status == 409 => Self::conflict(message.clone()),
            Api::Status {
                status,
                retry_after,
                ..
            } if *status == 429 => Self::rate_limited(retry_after.map(|d| d.as_secs())),

            // Not `not_permitted`: that code means a tool this server was not
            // started to offer, and its hint names a server flag. A refusal
            // from the control plane is about the credential instead, and
            // pointing an operator at the wrong switch is worse than no hint.
            Api::Status {
                status, message, ..
            } if matches!(status, 401 | 403) => Self::api_error(*status, message).with_hint(
                "Check that the control-plane credential is current and carries the \
                 scopes this call needs.",
            ),
            Api::Status {
                status, message, ..
            } => Self::api_error(*status, message),

            // A request that never became a response. The tailnet surface is
            // there and unreachable, which is what this code is for.
            Api::Transport { .. } => {
                Self::backend_unavailable("the control plane", &error.to_string())
            }

            Api::Timeout { request, budget } => Self::timeout(request, budget.as_secs(), ""),

            // Deliberately not `result_too_large`, whose message states an
            // exact size: the transport refuses before the whole body has
            // arrived, so the only honest claim is the one the cap gives.
            Api::TooLarge { .. } => {
                Self::new(ErrorCode::ResultTooLarge, error.to_string()).with_hint(TOO_LARGE_HINT)
            }

            // An answer arrived and could not be read. Nothing the caller did
            // is wrong, so there is no hint worth giving.
            Api::Malformed { .. } => Self::new(ErrorCode::ApiError, error.to_string()),

            // No call was made at all: the credential could not be turned into
            // one, or the client was built wrong.
            Api::Token(_) | Api::JwtFile { .. } | Api::Config(_) => {
                Self::backend_unavailable("the tailnet surface", &error.to_string())
            }
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ToolError {}

/// The result type every tool handler returns.
pub type ToolResult<T> = Result<T, ToolError>;

// ---------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------

/// What replaces a secret once it has been found.
pub const REDACTED: &str = "[redacted]";

/// Remove anything key-shaped from a string.
///
/// This is deliberately shape-based rather than value-based. We do hold the
/// credentials we were configured with, and [`Redactor`] scrubs those by value,
/// but the strings that pass through here mostly carry secrets we never had:
/// an auth key the model just minted, a key echoed back in an error, a token in
/// a URL the CLI printed. Only the shape is common to all of them.
///
/// Borrowed back unchanged when there is nothing to remove, which is the usual
/// case, so this is cheap to apply everywhere.
pub fn redact(input: &str) -> Cow<'_, str> {
    let mut out: Option<String> = None;
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut copied = 0;

    while i < bytes.len() {
        let hit = secret_at(input, i);
        match hit {
            Some((keep, end)) => {
                let out = out.get_or_insert_with(String::new);
                out.push_str(&input[copied..i + keep]);
                out.push_str(REDACTED);
                copied = end;
                i = end;
            }
            None => i += 1,
        }
    }

    match out {
        Some(mut out) => {
            out.push_str(&input[copied..]);
            Cow::Owned(out)
        }
        None => Cow::Borrowed(input),
    }
}

/// If a secret starts at `i`, return how many bytes of the match to keep as a
/// readable marker, and where the secret ends.
fn secret_at(input: &str, i: usize) -> Option<(usize, usize)> {
    // Only consider positions that begin a token, so `not-tskey-auth` in prose
    // is left alone.
    if i > 0 && is_token_byte(input.as_bytes()[i - 1]) {
        return None;
    }
    let rest = &input[i..];

    // `tskey-auth-…`, `tskey-api-…`, `tskey-client-…`, and the bare older form.
    // The prefix is kept so the reader can tell which kind of key was removed.
    for prefix in ["tskey-auth-", "tskey-api-", "tskey-client-", "tskey-"] {
        if let Some(tail) = rest.strip_prefix(prefix) {
            let len = token_len(tail);
            // A bare `tskey-` with nothing after it is not a key.
            if len == 0 {
                continue;
            }
            return Some((prefix.len(), i + prefix.len() + len));
        }
    }

    // `Authorization: Bearer <token>` in a captured header dump.
    for prefix in ["Bearer ", "bearer "] {
        if let Some(tail) = rest.strip_prefix(prefix) {
            let len = token_len(tail);
            if len == 0 {
                continue;
            }
            return Some((prefix.len(), i + prefix.len() + len));
        }
    }

    None
}

/// How many bytes at the start of `s` belong to a credential-shaped token.
fn token_len(s: &str) -> usize {
    s.bytes().take_while(|b| is_token_byte(*b)).count()
}

const fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.'
}

/// Scrubs known secret values in addition to key-shaped ones.
///
/// Built once at startup from whatever credentials were configured, then shared.
/// The literal pass matters for the OAuth client secret, which is the one
/// credential we hold that need not look like a Tailscale key.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    secrets: Vec<String>,
}

impl Redactor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a value to remove wherever it appears. Very short values are
    /// ignored: scrubbing a two-character "secret" would mangle every message.
    pub fn add_secret(&mut self, secret: impl Into<String>) {
        let secret = secret.into();
        if secret.len() >= 8 && !self.secrets.contains(&secret) {
            self.secrets.push(secret);
        }
    }

    #[must_use]
    pub fn with_secret(mut self, secret: impl Into<String>) -> Self {
        self.add_secret(secret);
        self
    }

    /// Shape-based redaction first, then the known values.
    pub fn apply<'a>(&self, input: &'a str) -> Cow<'a, str> {
        let mut current = redact(input);
        for secret in &self.secrets {
            if current.contains(secret.as_str()) {
                current = Cow::Owned(current.replace(secret.as_str(), REDACTED));
            }
        }
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    use tailscale_rest::ApiError;

    /// The shape every `Status` test starts from, so each one varies only the
    /// thing it is about.
    fn answered(status: u16, message: &str) -> ApiError {
        ApiError::Status {
            request: "GET /api/v2/tailnet/example.com/devices".to_owned(),
            status,
            message: message.to_owned(),
            retry_after: None,
        }
    }

    #[test]
    fn a_status_the_model_has_a_code_for_is_told_in_that_code() {
        for (status, expected) in [
            (404, ErrorCode::NotFound),
            (409, ErrorCode::Conflict),
            (429, ErrorCode::RateLimited),
        ] {
            let error = ToolError::from(answered(status, "no such device"));
            assert_eq!(error.code, expected, "HTTP {status}");
            assert_eq!(error.status, Some(status));
        }
    }

    #[test]
    fn a_status_the_model_has_no_code_for_keeps_its_number() {
        let error = ToolError::from(answered(422, "hostname is already taken"));
        assert_eq!(error.code, ErrorCode::ApiError);
        assert_eq!(error.status, Some(422));
        assert!(
            error.message.contains("hostname is already taken"),
            "the control plane's own words should survive: {}",
            error.message
        );
    }

    #[test]
    fn the_servers_backoff_becomes_the_wait_the_caller_is_told_about() {
        let error = ToolError::from(ApiError::Status {
            request: "GET /api/v2/tailnet/example.com/devices".to_owned(),
            status: 429,
            message: "slow down".to_owned(),
            retry_after: Some(Duration::from_secs(30)),
        });
        assert_eq!(error.hint.as_deref(), Some("Retry after 30s."));
    }

    #[test]
    fn a_refused_credential_is_not_reported_as_a_missing_switch() {
        // `not_permitted` names a server flag in its hint, and no flag makes a
        // rejected credential work. Sending an operator to one would be worse
        // than sending them nowhere.
        for status in [401, 403] {
            let error = ToolError::from(answered(status, "invalid key"));
            assert_eq!(error.code, ErrorCode::ApiError, "HTTP {status}");
            let hint = error.hint.as_deref().unwrap_or_default();
            assert!(
                hint.contains("credential") && hint.contains("scopes"),
                "HTTP {status} should point at the credential: {hint}"
            );
            assert!(
                !hint.contains("--"),
                "HTTP {status} should not name a server flag: {hint}"
            );
        }
    }

    #[test]
    fn an_answer_over_the_cap_says_so_with_the_narrowing_available() {
        let error = ToolError::from(ApiError::TooLarge {
            request: "GET /api/v2/tailnet/example.com/devices".to_owned(),
            cap: 1024,
        });
        assert_eq!(error.code, ErrorCode::ResultTooLarge);
        assert!(error.message.contains("1024"), "{}", error.message);
        assert_eq!(error.hint.as_deref(), Some(TOO_LARGE_HINT));
        // The same hint the tool-result cap gives, because it is the same cap.
        assert_eq!(
            error.hint,
            ToolError::result_too_large(2048, 1024).hint,
            "one cap should not have two answers"
        );
    }

    #[test]
    fn a_credential_that_could_not_be_used_is_the_surface_being_unavailable() {
        // None of these reached the network, so none of them is an API error:
        // what a caller needs to know is that the surface is not there.
        for error in [
            ApiError::Token("the token endpoint answered with 400".to_owned()),
            ApiError::JwtFile {
                path: std::path::PathBuf::from("/run/identity.jwt"),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
            },
            ApiError::Config("`http://elsewhere` is neither https nor loopback".to_owned()),
        ] {
            let reported = ToolError::from(error);
            assert_eq!(reported.code, ErrorCode::BackendUnavailable);
            assert!(reported.status.is_none());
        }
    }

    #[test]
    fn a_body_that_could_not_be_read_is_the_control_plane_being_wrong() {
        let source = serde_json::from_str::<i32>("not a number").expect_err("this does not parse");
        let error = ToolError::from(ApiError::Malformed {
            request: "GET /api/v2/tailnet/example.com/devices".to_owned(),
            source,
        });
        assert_eq!(error.code, ErrorCode::ApiError);
        // Nothing the caller did is wrong, so there is nothing to suggest.
        assert!(error.hint.is_none());
    }

    #[test]
    fn a_call_that_ran_out_of_budget_is_a_timeout_naming_the_budget() {
        let error = ToolError::from(ApiError::Timeout {
            request: "GET /api/v2/tailnet/example.com/devices".to_owned(),
            budget: Duration::from_secs(30),
        });
        assert_eq!(error.code, ErrorCode::Timeout);
        assert!(error.message.contains("30s"), "{}", error.message);
    }

    #[test]
    fn every_code_has_a_distinct_stable_name() {
        let mut names: Vec<&str> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(names.len(), 14, "the code vocabulary is fixed at fourteen");
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate error code name");
        for name in names {
            assert!(
                name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{name} is not snake_case"
            );
        }
    }

    #[test]
    fn codes_serialise_as_their_documented_strings() {
        for code in ErrorCode::ALL {
            let json = serde_json::to_string(code).expect("codes serialise");
            assert_eq!(json, format!("\"{}\"", code.as_str()));
        }
    }

    #[test]
    fn the_codes_an_operator_can_act_on_carry_a_hint() {
        let with_hints = [
            ToolError::not_permitted("tailscale_up", "--allow-write"),
            ToolError::unsupported_version("tailscale_x", "1.80", "1.70"),
            ToolError::unsupported_platform("tailscale_systray", "macos"),
            ToolError::result_too_large(2_000_000, 1_048_576),
            ToolError::conflict("the policy file changed"),
            ToolError::confirmation_required("tailscale_down", "disconnects this node"),
            ToolError::needs_operator(""),
            ToolError::rate_limited(Some(30)),
            ToolError::timeout("tailscale ping", 30, ""),
        ];
        for err in with_hints {
            assert!(err.hint.is_some(), "{} should carry a hint", err.code);
        }
    }

    #[test]
    fn a_command_that_hung_reports_what_it_was_waiting_on() {
        let silent = ToolError::timeout("tailscale funnel 3000", 30, "  ");
        assert_eq!(
            silent.message,
            "tailscale funnel 3000 did not finish within 30s"
        );

        let spoke = ToolError::timeout(
            "tailscale funnel 3000",
            30,
            "Funnel is not enabled on your tailnet.\nTo enable, visit:\n\n\thttps://login.example.com/f/funnel\n",
        );
        assert!(
            spoke.message.contains("https://login.example.com/f/funnel"),
            "the caller cannot act on what it was not told: {}",
            spoke.message
        );
        assert_ne!(
            spoke.hint, silent.hint,
            "a command that explained itself needs different advice from one that did not"
        );
    }

    #[test]
    fn absent_fields_are_omitted_from_the_wire_form() {
        let err = ToolError::invalid_args("port must be between 1 and 65535");
        let json = serde_json::to_value(&err).expect("errors serialise");
        let obj = json.as_object().expect("an object");
        assert_eq!(obj.len(), 2, "only code and message: {obj:?}");
        assert_eq!(obj["code"], "invalid_args");
    }

    #[test]
    fn key_shaped_values_are_removed_from_every_field() {
        let err = ToolError::cli_failed(
            "tailscale up",
            Some(1),
            "invalid key: tskey-auth-example1CNTRL-secretpart",
        );
        let stderr = err.stderr.expect("stderr is captured");
        assert!(!stderr.contains("secretpart"), "{stderr}");
        assert!(stderr.contains("tskey-auth-[redacted]"), "{stderr}");
    }

    #[test]
    fn a_command_killed_by_a_signal_reports_no_exit_code() {
        let err = ToolError::cli_failed("tailscale up", None, "");
        assert_eq!(err.exit_code, None);
        assert!(err.message.contains("terminated"), "{}", err.message);
    }

    #[test]
    fn each_key_shape_is_recognised() {
        for input in [
            "tskey-auth-example-def456",
            "tskey-api-example-def456",
            "tskey-client-example-def456",
            "tskey-exampledef456",
        ] {
            let out = redact(input);
            assert!(!out.contains("def456"), "{input} -> {out}");
            assert!(out.ends_with(REDACTED), "{input} -> {out}");
        }
    }

    #[test]
    fn bearer_tokens_are_removed() {
        let out = redact("Authorization: Bearer tskey-api-example-def");
        assert_eq!(out, "Authorization: Bearer [redacted]");
    }

    #[test]
    fn several_secrets_in_one_string_are_all_removed() {
        let out = redact("old tskey-auth-example-1 new tskey-auth-example-2 done");
        assert_eq!(
            out,
            "old tskey-auth-[redacted] new tskey-auth-[redacted] done"
        );
    }

    #[test]
    fn prose_that_merely_mentions_a_key_survives() {
        // No token follows, so there is nothing to remove.
        assert_eq!(
            redact("pass a tskey- prefixed value"),
            "pass a tskey- prefixed value"
        );
        // A word ending in the prefix is not the start of a token.
        assert_eq!(redact("see mytskey-auth-notes"), "see mytskey-auth-notes");
    }

    #[test]
    fn clean_strings_are_borrowed_not_copied() {
        assert!(matches!(redact("nothing to see here"), Cow::Borrowed(_)));
    }

    #[test]
    fn the_redactor_also_scrubs_values_it_was_given() {
        let r = Redactor::new().with_secret("an-oauth-client-secret-value");
        let out = r.apply("failed with an-oauth-client-secret-value and tskey-api-example-b");
        assert_eq!(
            out,
            format!("failed with {REDACTED} and tskey-api-{REDACTED}")
        );
    }

    #[test]
    fn the_redactor_ignores_values_too_short_to_be_secrets() {
        let r = Redactor::new().with_secret("abc");
        assert_eq!(
            r.apply("abc is a common substring"),
            "abc is a common substring"
        );
    }
}
