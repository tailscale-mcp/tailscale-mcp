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
use std::path::PathBuf;

use clap::Parser;

use crate::gating::{ConfigError, Preset, apply_toolset_modifiers};
use crate::meta::{Surface, Tier, Toolset};

/// The default ceiling on a single tool result, in bytes.
pub const DEFAULT_MAX_RESULT_BYTES: usize = 1 << 20;

/// What is logged when nothing says otherwise.
pub const DEFAULT_LOG_FILTER: &str = "warn,tailscale_mcp=info";

pub const PRESET_ENV: &str = "TAILSCALE_MCP_PRESET";
pub const TOOLSETS_ENV: &str = "TAILSCALE_MCP_TOOLSETS";
pub const ALLOW_WRITE_ENV: &str = "TAILSCALE_MCP_ALLOW_WRITE";
pub const ALLOW_DESTRUCTIVE_ENV: &str = "TAILSCALE_MCP_ALLOW_DESTRUCTIVE";
pub const NO_LOCAL_ENV: &str = "TAILSCALE_MCP_NO_LOCAL";
pub const NO_TAILNET_ENV: &str = "TAILSCALE_MCP_NO_TAILNET";
pub const CLI_PATH_ENV: &str = "TAILSCALE_MCP_CLI_PATH";
pub const MAX_RESULT_BYTES_ENV: &str = "TAILSCALE_MCP_MAX_RESULT_BYTES";
pub const LOG_ENV: &str = "TAILSCALE_MCP_LOG";
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
    #[arg(long, value_name = "NAME")]
    pub preset: Option<String>,

    /// Toolsets to offer. A bare list replaces the preset's selection; a list
    /// where every entry begins with `+` or `-` adjusts it.
    #[arg(long, value_name = "LIST", allow_hyphen_values = true)]
    pub toolsets: Option<String>,

    /// Permit tools that change configuration.
    #[arg(long)]
    pub allow_write: bool,

    /// Permit tools that remove or expose something. Implies --allow-write.
    #[arg(long)]
    pub allow_destructive: bool,

    /// Do not offer the local surface, even if the CLI is present.
    #[arg(long)]
    pub no_local: bool,

    /// Do not offer the tailnet surface, even if a credential is present.
    #[arg(long)]
    pub no_tailnet: bool,

    /// Where the `tailscale` binary is, when it is not on the path.
    #[arg(long, value_name = "PATH")]
    pub cli_path: Option<PathBuf>,

    /// Refuse a tool result larger than this many bytes.
    #[arg(long, value_name = "BYTES")]
    pub max_result_bytes: Option<usize>,

    /// Logging filter, in the `tracing` syntax. Logs go to standard error.
    #[arg(long, value_name = "FILTER")]
    pub log: Option<String>,
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
            log_filter: cli
                .log
                .or_else(|| get(LOG_ENV))
                .unwrap_or_else(|| DEFAULT_LOG_FILTER.to_owned()),
        })
    }

    /// Whether the operator switched this surface off.
    pub fn is_disabled(&self, surface: Surface) -> bool {
        self.disabled.contains(&surface)
    }
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
        assert_eq!(config.log_filter, "debug");
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
        assert_eq!(config.log_filter, "trace");
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
        assert_eq!(config.log_filter, DEFAULT_LOG_FILTER);
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

    #[test]
    fn the_documented_variables_are_the_ones_that_are_read() {
        // A variable added to the resolution but not to the list would be
        // invisible to the diagnosis subcommand and to the help text.
        for var in ENV_VARS {
            assert!(
                LONG_ABOUT.contains(var),
                "{var} is read but not documented in the help"
            );
        }
        assert_eq!(ENV_VARS.len(), 10);
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
}
