//! Turning flags and environment variables into a settled configuration.
//!
//! Environment matters more here than in most command-line programs: an MCP
//! client launches this server from a JSON configuration file that sets
//! environment variables, and often cannot pass arguments at all. So every
//! setting has both forms, the command line wins, and the resolution is a
//! plain function over a source of variables rather than something clap does
//! behind our back — which is what makes it testable without mutating the
//! environment of a running test binary.

use std::collections::BTreeSet;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;

use clap::Parser;

use crate::gating::{ConfigError, Preset, apply_toolset_modifiers};
use crate::meta::{Surface, Tier, Toolset};

/// The default ceiling on a single tool result, in bytes.
pub const DEFAULT_MAX_RESULT_BYTES: usize = 1 << 20;

/// What is logged when nothing says otherwise.
pub const DEFAULT_LOG_FILTER: &str = "warn,tailscale_mcp=info";

/// The most the MCP SDK is allowed to say unless it is asked for by name.
///
/// `rmcp` traces whole JSON-RPC messages at `TRACE` and `DEBUG`, results
/// included — so an operator who set `--log=debug` to follow this server's own
/// work would also write every minted auth key, every OAuth client secret and
/// every invite URL to standard error, without asking for any of that (Q79).
///
/// So the SDK is capped at `info` on top of whatever was asked for. An
/// operator who genuinely wants the wire can still have it, by naming the
/// target — `--log=info,rmcp=trace` — which is a deliberate act rather than a
/// side effect of turning up the volume.
const SDK_CAP: &str = "rmcp=info";

/// Whatever was asked for, with `rmcp=info` on the end.
///
/// Left alone if the filter already mentions the SDK: that is an operator who
/// has said what they want from it, and overriding a deliberate choice with a
/// default would be worse than the exposure this prevents.
pub fn bounded_log_filter(requested: &str) -> String {
    if requested.split(',').any(|directive| {
        directive
            .split('=')
            .next()
            .is_some_and(|target| target.trim() == "rmcp")
    }) {
        return requested.to_owned();
    }
    format!("{requested},{SDK_CAP}")
}

pub const PRESET_ENV: &str = "TAILSCALE_MCP_PRESET";
pub const TOOLSETS_ENV: &str = "TAILSCALE_MCP_TOOLSETS";
pub const ALLOW_WRITE_ENV: &str = "TAILSCALE_MCP_ALLOW_WRITE";
pub const ALLOW_DESTRUCTIVE_ENV: &str = "TAILSCALE_MCP_ALLOW_DESTRUCTIVE";
pub const NO_LOCAL_ENV: &str = "TAILSCALE_MCP_NO_LOCAL";
pub const NO_TAILNET_ENV: &str = "TAILSCALE_MCP_NO_TAILNET";
pub const CLI_PATH_ENV: &str = "TAILSCALE_MCP_CLI_PATH";
pub const MAX_RESULT_BYTES_ENV: &str = "TAILSCALE_MCP_MAX_RESULT_BYTES";
pub const LOG_ENV: &str = "TAILSCALE_MCP_LOG";
/// The bearer token the HTTP transport requires.
///
/// A variable and no flag, because a token on a command line is a token in
/// every process listing on the machine.
pub const HTTP_TOKEN_ENV: &str = "TAILSCALE_MCP_HTTP_TOKEN";
/// Serve HTTP with no token at all.
pub const HTTP_NO_AUTH_ENV: &str = "TAILSCALE_MCP_HTTP_NO_AUTH";
/// Serve HTTP without sessions.
pub const HTTP_STATELESS_ENV: &str = "TAILSCALE_MCP_HTTP_STATELESS";
/// Where the control-plane calls go. Deliberately without a command-line form:
/// it exists so the tests can point at a fake and so a staging control plane
/// can be reached, and neither belongs among the flags. It is documented all
/// the same — a setting that redirects every credential this server holds is
/// worse hidden than explained.
pub const API_BASE_URL_ENV: &str = "TAILSCALE_MCP_API_BASE_URL";

/// Every variable this module reads, for the diagnosis subcommand and the
/// documentation to stay in step with the code.
pub const ENV_VARS: &[&str] = &[
    PRESET_ENV,
    TOOLSETS_ENV,
    ALLOW_WRITE_ENV,
    ALLOW_DESTRUCTIVE_ENV,
    NO_LOCAL_ENV,
    NO_TAILNET_ENV,
    CLI_PATH_ENV,
    MAX_RESULT_BYTES_ENV,
    LOG_ENV,
    HTTP_TOKEN_ENV,
    HTTP_NO_AUTH_ENV,
    HTTP_STATELESS_ENV,
    API_BASE_URL_ENV,
];

const LONG_ABOUT: &str = "\
An MCP server for Tailscale. The local node is driven through the `tailscale`
command-line interface; the tailnet is driven through the control-plane REST
API. Tools are read-only unless a tier is permitted, and hidden rather than
refused when they are not.

Every option below can also be set through the environment, which is what an
MCP client configuration usually does. An option given on the command line
wins over the matching variable.

  TAILSCALE_MCP_PRESET             --preset
  TAILSCALE_MCP_TOOLSETS           --toolsets
  TAILSCALE_MCP_ALLOW_WRITE        --allow-write
  TAILSCALE_MCP_ALLOW_DESTRUCTIVE  --allow-destructive
  TAILSCALE_MCP_NO_LOCAL           --no-local
  TAILSCALE_MCP_NO_TAILNET         --no-tailnet
  TAILSCALE_MCP_CLI_PATH           --cli-path
  TAILSCALE_MCP_MAX_RESULT_BYTES   --max-result-bytes
  TAILSCALE_MCP_LOG                --log
  TAILSCALE_MCP_HTTP_NO_AUTH       --http-no-auth
  TAILSCALE_MCP_HTTP_STATELESS     --http-stateless

--http serves over Streamable HTTP instead of stdio, on 127.0.0.1:8449 unless
given an address, with --http-allow-host and --http-allow-origin to widen what
it answers for. TAILSCALE_MCP_HTTP_TOKEN is the bearer token callers must
present; it has no command-line form, because an argument is readable by every
process on this machine. A bind anywhere but loopback needs either that variable
or --http-no-auth.

TAILSCALE_TAILNET names the tailnet the control-plane tools act on; without it
they act on the one the credential belongs to. TAILSCALE_MCP_API_BASE_URL sends
the control-plane calls somewhere other than https://api.tailscale.com. It is
there so the test suite can reach a fake on this machine, and has no
command-line form. An address is accepted only over https or to this machine,
and never with a username or password in it.

Credentials for the tailnet surface are read from TAILSCALE_API_KEY, or from
TAILSCALE_OAUTH_CLIENT_ID and TAILSCALE_OAUTH_CLIENT_SECRET, or from
TAILSCALE_OAUTH_JWT_FILE, in that order. They have no command-line form: a
secret on an argument list is visible to every process on the machine.";

/// The command line, before the environment has been folded in.
#[derive(Debug, Clone, Default, Parser)]
#[command(name = "tailscale-mcp", version, about, long_about = LONG_ABOUT)]
pub struct Cli {
    /// Which group of toolsets to start from: minimal, core or full.
    #[arg(long, global = true, value_name = "NAME")]
    pub preset: Option<String>,

    /// Toolsets to offer. A bare list replaces the preset's selection; a list
    /// where every entry begins with `+` or `-` adjusts it.
    #[arg(long, global = true, value_name = "LIST", allow_hyphen_values = true)]
    pub toolsets: Option<String>,

    /// Permit tools that change configuration.
    #[arg(long, global = true)]
    pub allow_write: bool,

    /// Permit tools that remove or expose something. Implies --allow-write.
    #[arg(long, global = true)]
    pub allow_destructive: bool,

    /// Do not offer the local surface, even if the CLI is present.
    #[arg(long, global = true)]
    pub no_local: bool,

    /// Do not offer the tailnet surface, even if a credential is present.
    #[arg(long, global = true)]
    pub no_tailnet: bool,

    /// Where the `tailscale` binary is, when it is not on the path.
    #[arg(long, global = true, value_name = "PATH")]
    pub cli_path: Option<PathBuf>,

    /// Refuse a tool result larger than this many bytes.
    #[arg(long, global = true, value_name = "BYTES")]
    pub max_result_bytes: Option<usize>,

    /// Logging filter, in the `tracing` syntax. Logs go to standard error.
    #[arg(long, global = true, value_name = "FILTER")]
    pub log: Option<String>,

    /// Serve MCP over HTTP at this address instead of over stdio. Defaults to
    /// 127.0.0.1:8449 when the flag is given with no address.
    #[arg(long, value_name = "ADDR", num_args = 0..=1, default_missing_value = crate::http::DEFAULT_BIND)]
    pub http: Option<String>,

    /// Serve HTTP with no token at all. Required to bind anywhere but
    /// loopback without one.
    #[arg(long)]
    pub http_no_auth: bool,

    /// Also answer for this Host header. Repeatable. Loopback and this node's
    /// own tailnet names are always allowed.
    #[arg(long, value_name = "HOST")]
    pub http_allow_host: Vec<String>,

    /// Answer requests from this browser origin. Repeatable. Without it, a
    /// request carrying any Origin is refused.
    #[arg(long, value_name = "ORIGIN")]
    pub http_allow_origin: Vec<String>,

    /// Serve HTTP without sessions, where the negotiated protocol version
    /// still has them. From 2026-07-28 the protocol has no sessions and this
    /// changes nothing.
    #[arg(long)]
    pub http_stateless: bool,

    /// Ask this binary a question instead of serving. Without one it serves.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The things this binary does other than serve.
#[derive(Debug, Clone, clap::Subcommand)]
pub enum Command {
    /// Check the CLI, the credential and the control plane, and report each.
    ///
    /// Exits non-zero when a check fails, so that a script can act on it.
    Diagnose {
        /// Report as JSON rather than as a table.
        #[arg(long)]
        json: bool,
    },
    /// Print what this preset and tier would offer, without serving.
    Tools {
        /// Report as JSON rather than as a table.
        #[arg(long)]
        json: bool,
    },
    /// Print this server's version and the protocol versions it speaks.
    Version,
    /// Print a configuration snippet for an MCP client. Writes nothing.
    Setup {
        /// Which client the snippet is for.
        #[arg(value_enum)]
        client: Client,
    },
    /// Validate a policy file, or deploy it. Quiet on success.
    Policy {
        #[command(subcommand)]
        action: PolicyCommand,
    },
}

/// What to do with a policy file.
#[derive(Debug, Clone, clap::Subcommand)]
pub enum PolicyCommand {
    /// Check that the control plane would accept this file. Changes nothing.
    Check {
        /// The policy file, HuJSON or JSON.
        file: PathBuf,
    },
    /// Write this file as the tailnet policy, guarded by the version
    /// identifier read immediately before.
    Deploy {
        /// The policy file, HuJSON or JSON.
        file: PathBuf,
    },
}

/// The clients `setup` knows how to write a snippet for.
///
/// One enum and not two: clap derives the parsing and the list of variants
/// from it, so a sixth client is offered, parsed and tested from one place
/// rather than from four (Q95).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Client {
    ClaudeCode,
    ClaudeDesktop,
    Vscode,
    Cursor,
    Zed,
}

/// How to serve MCP over HTTP, once the operator has asked for it.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// Where to listen.
    pub bind: SocketAddr,
    /// The bearer token, or `None` when the operator said there is none.
    pub token: Option<tailscale_rest::Secret>,
    /// Host headers to answer for beyond the ones always allowed.
    pub allow_hosts: Vec<String>,
    /// Browser origins to answer for.
    pub allow_origins: Vec<String>,
    /// Whether to keep sessions.
    pub stateful: bool,
}

/// The configuration the server actually runs with.
#[derive(Debug, Clone)]
pub struct Config {
    /// The preset the selection started from, kept for diagnostics.
    pub preset: Preset,
    /// The toolsets to offer, after any adjustment.
    pub toolsets: BTreeSet<Toolset>,
    /// The most dangerous tier permitted.
    pub max_tier: Tier,
    /// Surfaces the operator switched off. A surface can also be unavailable
    /// because its backend is missing, which is decided at startup, not here.
    pub disabled: BTreeSet<Surface>,
    pub cli_path: Option<PathBuf>,
    pub max_result_bytes: usize,
    /// Where the control-plane calls go. The real control plane unless a test
    /// or a staging environment says otherwise; validated when the client is
    /// built, because that is where the rule about what is safe lives.
    pub api_base_url: String,
    /// The tailnet a control-plane path means when the caller does not name
    /// one.
    pub tailnet: String,
    pub log_filter: String,
    /// Serve over HTTP instead of stdio, when the operator asked for it.
    pub http: Option<HttpConfig>,
}

impl Config {
    /// Resolve the command line against the process environment.
    pub fn resolve(cli: Cli) -> Result<Self, ConfigError> {
        Self::resolve_with(cli, |key| std::env::var(key).ok())
    }

    /// Resolve the command line against an arbitrary source of variables.
    pub fn resolve_with(
        cli: Cli,
        source: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, ConfigError> {
        let get = |key: &str| {
            source(key)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let flag = |set: bool, key: &str| -> Result<bool, ConfigError> {
            if set {
                return Ok(true);
            }
            match get(key) {
                None => Ok(false),
                Some(raw) => parse_bool(&raw).ok_or_else(|| ConfigError::InvalidValue {
                    setting: key.to_owned(),
                    value: raw,
                    expected: "a boolean (1, true, yes, on, or their negatives)",
                }),
            }
        };

        let preset = match cli.preset.or_else(|| get(PRESET_ENV)) {
            Some(name) => Preset::parse(&name)?,
            None => Preset::default(),
        };

        let spec = cli.toolsets.or_else(|| get(TOOLSETS_ENV));
        let toolsets = match spec.as_deref() {
            Some(spec) => apply_toolset_modifiers(preset.toolsets(), spec)?,
            None => preset.toolsets(),
        };

        let allow_destructive = flag(cli.allow_destructive, ALLOW_DESTRUCTIVE_ENV)?;
        let allow_write = allow_destructive || flag(cli.allow_write, ALLOW_WRITE_ENV)?;
        let max_tier = match (allow_destructive, allow_write) {
            (true, _) => Tier::Destructive,
            (_, true) => Tier::Write,
            _ => Tier::Read,
        };

        let mut disabled = BTreeSet::new();
        if flag(cli.no_local, NO_LOCAL_ENV)? {
            disabled.insert(Surface::Local);
        }
        if flag(cli.no_tailnet, NO_TAILNET_ENV)? {
            disabled.insert(Surface::Tailnet);
        }

        let max_result_bytes = match cli.max_result_bytes {
            // Judged the same as the variable below it: a cap of zero rejects
            // every answer, and does it from inside the control-plane client,
            // where it reads as the server being misconfigured rather than as
            // the number the operator typed.
            Some(0) => {
                return Err(ConfigError::InvalidValue {
                    setting: "--max-result-bytes".to_owned(),
                    value: "0".to_owned(),
                    expected: "a positive number of bytes",
                });
            }
            Some(n) => n,
            None => match get(MAX_RESULT_BYTES_ENV) {
                Some(raw) => raw
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| ConfigError::InvalidValue {
                        setting: MAX_RESULT_BYTES_ENV.to_owned(),
                        value: raw,
                        expected: "a positive number of bytes",
                    })?,
                None => DEFAULT_MAX_RESULT_BYTES,
            },
        };

        let http = match cli.http {
            None => None,
            Some(address) => Some(checked_http(
                HttpConfig {
                    bind: http_bind(&address)?,
                    // No flag for the token, deliberately: an argument is
                    // visible to every process on the machine, and this server
                    // has never had a way to put a secret on its own command
                    // line (Q91).
                    token: get(HTTP_TOKEN_ENV).map(tailscale_rest::Secret::new),
                    allow_hosts: cli.http_allow_host,
                    allow_origins: cli.http_allow_origin,
                    stateful: !flag(cli.http_stateless, HTTP_STATELESS_ENV)?,
                },
                &address,
                flag(cli.http_no_auth, HTTP_NO_AUTH_ENV)?,
            )?),
        };

        Ok(Self {
            preset,
            toolsets,
            max_tier,
            disabled,
            cli_path: cli
                .cli_path
                .or_else(|| get(CLI_PATH_ENV).map(PathBuf::from)),
            max_result_bytes,
            api_base_url: get(API_BASE_URL_ENV)
                .unwrap_or_else(|| tailscale_rest::DEFAULT_BASE_URL.to_owned()),
            tailnet: tailscale_rest::credentials::tailnet_from_source(&source),
            log_filter: bounded_log_filter(
                &cli.log
                    .or_else(|| get(LOG_ENV))
                    .unwrap_or_else(|| DEFAULT_LOG_FILTER.to_owned()),
            ),
            http,
        })
    }

    /// Whether the operator switched this surface off.
    pub fn is_disabled(&self, surface: Surface) -> bool {
        self.disabled.contains(&surface)
    }

    /// The settings that differ from what this server would do on its own, as
    /// the variables that would reproduce them.
    ///
    /// This is `resolve` run backwards, which is why it lives beside it: every
    /// field it reads and every variable it names is this module's, and a
    /// setting added to one without the other is a snippet that does not
    /// reproduce the server it was printed from.
    ///
    /// Only what the operator changed. A snippet spelling out every default
    /// would be a snippet nobody reads, and every value in it would go stale
    /// the day a default changed.
    pub fn changed_settings(&self) -> Vec<(&'static str, String)> {
        let mut chosen = Vec::new();
        if self.preset != Preset::default() {
            chosen.push((PRESET_ENV, self.preset.as_str().to_owned()));
        }
        if self.toolsets != self.preset.toolsets() {
            chosen.push((
                TOOLSETS_ENV,
                self.toolsets
                    .iter()
                    .map(|toolset| toolset.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            ));
        }
        match self.max_tier {
            Tier::Destructive => chosen.push((ALLOW_DESTRUCTIVE_ENV, "true".to_owned())),
            Tier::Write => chosen.push((ALLOW_WRITE_ENV, "true".to_owned())),
            Tier::Read => {}
        }
        if self.is_disabled(Surface::Local) {
            chosen.push((NO_LOCAL_ENV, "true".to_owned()));
        }
        if self.is_disabled(Surface::Tailnet) {
            chosen.push((NO_TAILNET_ENV, "true".to_owned()));
        }
        if let Some(path) = &self.cli_path {
            chosen.push((CLI_PATH_ENV, path.display().to_string()));
        }
        if self.max_result_bytes != DEFAULT_MAX_RESULT_BYTES {
            chosen.push((MAX_RESULT_BYTES_ENV, self.max_result_bytes.to_string()));
        }
        // Against the bounded default, not the raw one: `resolve` bounds every
        // filter it is given, including the default, so comparing with the raw
        // string would report the default as a change on every run.
        if self.log_filter != bounded_log_filter(DEFAULT_LOG_FILTER) {
            chosen.push((LOG_ENV, self.log_filter.clone()));
        }
        chosen
    }
}

/// Variables `changed_settings` deliberately never carries into a snippet.
///
/// The three HTTP ones because a client launches this binary and talks to it
/// over stdio, so a snippet that turned on the HTTP transport would describe a
/// server the client cannot reach; and the base address because it exists for
/// the test suite to point at a fake and has no place in an operator's
/// configuration (Q96).
pub const NOT_IN_A_SNIPPET: &[&str] = &[
    HTTP_TOKEN_ENV,
    HTTP_NO_AUTH_ENV,
    HTTP_STATELESS_ENV,
    API_BASE_URL_ENV,
];

/// Resolve the HTTP options, refusing the combination that would publish an
/// unauthenticated control plane by accident.
///
/// A token is optional on loopback, where the operating system has already
/// decided who can reach the socket, and required everywhere else. The
/// no-authentication flag is the only way past that, and is a flag rather than
/// an omission on purpose: serving a tailnet address to anyone who asks should
/// be something an operator did, not something that happened.
fn checked_http(
    settings: HttpConfig,
    address: &str,
    no_auth: bool,
) -> Result<HttpConfig, ConfigError> {
    if no_auth {
        return Ok(HttpConfig {
            token: None,
            ..settings
        });
    }
    // The unspecified address — `0.0.0.0` or `::` — is emphatically not
    // loopback: it is every interface the machine has, which is the case the
    // token exists for.
    if settings.token.is_none() && !settings.bind.ip().is_loopback() {
        return Err(ConfigError::InvalidValue {
            setting: "--http".to_owned(),
            value: address.to_owned(),
            // The template this renders into reads "…which is not {expected}",
            // so what follows has to be the one thing the address failed to
            // be. The two ways past it are remedies rather than alternative
            // things the address could have been, and reading as though they
            // were made the sentence say an address is not a token.
            expected: "a loopback address \u{2014} set a token in \
                       TAILSCALE_MCP_HTTP_TOKEN, or pass --http-no-auth to say \
                       the address really should answer anyone who asks",
        });
    }
    Ok(settings)
}

/// The address `--http` was given, resolved.
fn http_bind(address: &str) -> Result<SocketAddr, ConfigError> {
    address
        .to_socket_addrs()
        .ok()
        .and_then(|mut found| found.next())
        .ok_or_else(|| ConfigError::InvalidValue {
            setting: "--http".to_owned(),
            value: address.to_owned(),
            expected: "an address and port, such as 127.0.0.1:8449",
        })
}

/// The spellings people actually write in a container environment file.
fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" | "" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |key| map.get(key).cloned()
    }

    fn resolve(cli: Cli, pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        Config::resolve_with(cli, env(pairs))
    }

    #[test]
    fn the_defaults_are_the_core_preset_and_read_only() {
        let config = resolve(Cli::default(), &[]).expect("defaults resolve");
        assert_eq!(config.preset, Preset::Core);
        assert_eq!(config.toolsets, Preset::Core.toolsets());
        assert_eq!(config.max_tier, Tier::Read);
        assert!(config.disabled.is_empty());
        assert_eq!(config.max_result_bytes, DEFAULT_MAX_RESULT_BYTES);
    }

    #[test]
    fn the_environment_configures_a_client_that_cannot_pass_arguments() {
        let config = resolve(
            Cli::default(),
            &[
                (PRESET_ENV, "full"),
                (ALLOW_WRITE_ENV, "true"),
                (NO_TAILNET_ENV, "1"),
                (MAX_RESULT_BYTES_ENV, "4096"),
                (LOG_ENV, "debug"),
            ],
        )
        .expect("resolves");
        assert_eq!(config.preset, Preset::Full);
        assert_eq!(config.max_tier, Tier::Write);
        assert!(config.is_disabled(Surface::Tailnet));
        assert!(!config.is_disabled(Surface::Local));
        assert_eq!(config.max_result_bytes, 4096);
        assert_eq!(config.log_filter, "debug,rmcp=info");
    }

    #[test]
    fn the_command_line_wins_over_the_environment() {
        let cli = Cli {
            preset: Some("minimal".to_owned()),
            log: Some("trace".to_owned()),
            max_result_bytes: Some(1024),
            ..Cli::default()
        };
        let config = resolve(
            cli,
            &[
                (PRESET_ENV, "full"),
                (LOG_ENV, "debug"),
                (MAX_RESULT_BYTES_ENV, "4096"),
            ],
        )
        .expect("resolves");
        assert_eq!(config.preset, Preset::Minimal);
        assert_eq!(config.log_filter, "trace,rmcp=info");
        assert_eq!(config.max_result_bytes, 1024);
    }

    #[test]
    fn a_flag_set_on_the_command_line_is_not_undone_by_the_environment() {
        // The environment cannot say "false" loudly enough to override an
        // explicit flag: the flag's presence is the operator's intent.
        let cli = Cli {
            allow_write: true,
            ..Cli::default()
        };
        let config = resolve(cli, &[(ALLOW_WRITE_ENV, "false")]).expect("resolves");
        assert_eq!(config.max_tier, Tier::Write);
    }

    #[test]
    fn permitting_destruction_permits_writing() {
        let cli = Cli {
            allow_destructive: true,
            ..Cli::default()
        };
        let config = resolve(cli, &[]).expect("resolves");
        assert_eq!(config.max_tier, Tier::Destructive);
        assert!(config.max_tier >= Tier::Write);
    }

    #[test]
    fn toolsets_adjust_the_preset() {
        let cli = Cli {
            toolsets: Some("+local-debug,-tailnet-dns".to_owned()),
            ..Cli::default()
        };
        let config = resolve(cli, &[]).expect("resolves");
        assert!(config.toolsets.contains(&Toolset::LocalDebug));
        assert!(!config.toolsets.contains(&Toolset::TailnetDns));
        assert!(config.toolsets.contains(&Toolset::LocalStatus));
    }

    #[test]
    fn a_bare_toolset_list_replaces_the_preset() {
        let config = resolve(
            Cli::default(),
            &[(PRESET_ENV, "full"), (TOOLSETS_ENV, "local-status")],
        )
        .expect("resolves");
        assert_eq!(config.toolsets, BTreeSet::from([Toolset::LocalStatus]));
        // The preset is still reported, because it is what the selection
        // started from and diagnostics should say so.
        assert_eq!(config.preset, Preset::Full);
    }

    #[test]
    fn a_setting_that_cannot_be_understood_stops_the_server() {
        let cases: [(&str, &str); 4] = [
            (PRESET_ENV, "everything"),
            (TOOLSETS_ENV, "not-a-toolset"),
            (ALLOW_WRITE_ENV, "perhaps"),
            (MAX_RESULT_BYTES_ENV, "0"),
        ];
        for (key, value) in cases {
            let err = resolve(Cli::default(), &[(key, value)])
                .expect_err("{key}={value} should be refused");
            let message = err.to_string();
            assert!(
                message.contains(value) || message.contains("unknown"),
                "{key}={value} produced {message}"
            );
        }
    }

    #[test]
    fn a_blank_variable_is_the_same_as_an_unset_one() {
        let config = resolve(
            Cli::default(),
            &[(PRESET_ENV, "  "), (LOG_ENV, ""), (TOOLSETS_ENV, " ")],
        )
        .expect("resolves");
        assert_eq!(config.preset, Preset::Core);
        assert_eq!(config.log_filter, bounded_log_filter(DEFAULT_LOG_FILTER));
    }

    #[test]
    fn turning_up_the_volume_does_not_turn_on_the_wire() {
        // The SDK traces whole messages, results included, so `debug` alone
        // would write every minted secret to standard error (Q79).
        assert_eq!(bounded_log_filter("debug"), "debug,rmcp=info");
        assert_eq!(
            bounded_log_filter(DEFAULT_LOG_FILTER),
            "warn,tailscale_mcp=info,rmcp=info"
        );
        // Unless it is asked for by name, which is a deliberate act.
        assert_eq!(bounded_log_filter("info,rmcp=trace"), "info,rmcp=trace");
        assert_eq!(bounded_log_filter("rmcp=debug"), "rmcp=debug");
    }

    #[test]
    fn the_boolean_spellings_people_write_are_all_understood() {
        for raw in ["1", "true", "TRUE", "yes", "on"] {
            let config = resolve(Cli::default(), &[(ALLOW_WRITE_ENV, raw)]).expect("resolves");
            assert_eq!(config.max_tier, Tier::Write, "{raw}");
        }
        for raw in ["0", "false", "no", "off"] {
            let config = resolve(Cli::default(), &[(ALLOW_WRITE_ENV, raw)]).expect("resolves");
            assert_eq!(config.max_tier, Tier::Read, "{raw}");
        }
    }

    /// This module's own source, so that the test below can hold the list to
    /// the code rather than to a number somebody has to remember to change.
    const SOURCE: &str = include_str!("config.rs");

    #[test]
    fn the_documented_variables_are_the_ones_that_are_read() {
        // A variable added to the resolution but not to the list would be
        // invisible to the diagnosis subcommand and to the help text. The
        // previous version of this test pinned `ENV_VARS.len()` to a number,
        // which is a check that passes while the list is wrong: three
        // variables were read and undocumented and it said nothing.
        for var in ENV_VARS {
            assert!(
                LONG_ABOUT.contains(var),
                "{var} is read but not documented in the help"
            );
        }

        // Constant name to the variable it names, from the declarations above.
        let declared: std::collections::HashMap<&str, &str> = SOURCE
            .lines()
            .filter_map(|line| {
                let line = line.trim().strip_prefix("pub const ")?;
                let (name, rest) = line.split_once(": &str = \"")?;
                name.ends_with("_ENV")
                    .then(|| (name, rest.split('"').next().unwrap_or_default()))
            })
            .collect();
        assert!(
            declared.len() >= ENV_VARS.len(),
            "every listed variable should have a declaration to be found"
        );

        // Every `*_ENV` constant the resolution actually reads.
        let resolution = SOURCE
            .split_once("pub fn resolve_with(")
            .and_then(|(_, rest)| rest.split_once("\n    /// Whether the operator switched"))
            .map_or(SOURCE, |(body, _)| body);
        for (name, var) in &declared {
            let read = resolution.contains(&format!("{name})"))
                || resolution.contains(&format!("{name},"));
            if read {
                assert!(
                    ENV_VARS.contains(var),
                    "{name} is read by the resolution but is not in ENV_VARS, so nothing \
                     documents it and the diagnosis subcommand will not check it"
                );
            }
        }
    }

    #[test]
    fn the_parser_accepts_the_flags_the_help_documents() {
        let cli = Cli::try_parse_from([
            "tailscale-mcp",
            "--preset",
            "full",
            "--toolsets",
            "+local-debug",
            "--allow-destructive",
            "--no-local",
            "--cli-path",
            "/opt/tailscale",
            "--max-result-bytes",
            "2048",
            "--log",
            "debug",
        ])
        .expect("the flags parse");
        assert_eq!(cli.preset.as_deref(), Some("full"));
        assert_eq!(cli.toolsets.as_deref(), Some("+local-debug"));
        assert!(cli.allow_destructive);
        assert!(cli.no_local);
        assert_eq!(cli.cli_path, Some(PathBuf::from("/opt/tailscale")));
        assert_eq!(cli.max_result_bytes, Some(2048));
    }

    #[test]
    fn a_leading_minus_in_a_toolset_list_is_not_read_as_a_flag() {
        let cli = Cli::try_parse_from(["tailscale-mcp", "--toolsets", "-tailnet-dns"])
            .expect("a removal parses");
        assert_eq!(cli.toolsets.as_deref(), Some("-tailnet-dns"));
    }

    #[test]
    fn the_command_line_offers_no_way_to_pass_a_secret() {
        use clap::CommandFactory as _;
        for arg in Cli::command().get_arguments() {
            let name = arg.get_id().as_str();
            assert!(
                !["key", "secret", "token", "password"]
                    .iter()
                    .any(|bad| name.contains(bad)),
                "`--{name}` would put a secret on the argument list"
            );
        }
    }

    /// Every `InvalidValue` reads as one sentence.
    ///
    /// `ConfigError::InvalidValue` renders as "… which is not {expected}", so
    /// `expected` has to be the thing the value failed to be. The HTTP guard
    /// listed its two remedies there instead — a token and a flag — and the
    /// sentence came out saying an address was not an environment variable.
    /// A remedy belongs after the clause ends, which is what the dash marks.
    #[test]
    fn an_invalid_value_says_what_the_value_should_have_been() {
        let phrases = [
            checked_http(
                HttpConfig {
                    bind: "0.0.0.0:8449".parse().expect("an address"),
                    token: None,
                    allow_hosts: Vec::new(),
                    allow_origins: Vec::new(),
                    stateful: true,
                },
                "0.0.0.0:8449",
                false,
            )
            .expect_err("an unauthenticated public bind is refused"),
            http_bind("not-an-address").expect_err("a bad address is refused"),
        ];
        for error in phrases {
            let rendered = error.to_string();
            let (_, expected) = rendered
                .split_once("which is not ")
                .unwrap_or_else(|| panic!("the template should be intact: {rendered}"));
            // Where the clause ends, the remedies may begin; before that, the
            // sentence is still saying what the value was not.
            let clause = expected
                .split(['\u{2014}', ';'])
                .next()
                .expect("a first clause");
            assert!(
                !clause.contains(" or "),
                "`which is not {clause}` offers alternatives where it should name \
                 one thing; put the remedies after a dash: {rendered}"
            );
            assert!(
                !clause.contains("--") && !clause.contains("TAILSCALE_"),
                "`which is not {clause}` says the value is not a flag or a variable, \
                 which is not what failed: {rendered}"
            );
        }
    }
}
