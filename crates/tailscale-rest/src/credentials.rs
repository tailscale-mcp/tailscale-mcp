//! Finding a control-plane credential in the environment.
//!
//! Three shapes are accepted, in a fixed order of precedence: an API key, an
//! OAuth client, and a federated identity backed by a JWT on disk. The order is
//! fixed rather than "whichever is set" so that an operator who leaves an old
//! key in a shell profile gets a predictable answer instead of a lottery.

use std::path::PathBuf;

use crate::secret::Secret;

pub const API_KEY_ENV: &str = "TAILSCALE_API_KEY";
pub const OAUTH_CLIENT_ID_ENV: &str = "TAILSCALE_OAUTH_CLIENT_ID";
pub const OAUTH_CLIENT_SECRET_ENV: &str = "TAILSCALE_OAUTH_CLIENT_SECRET";
pub const OAUTH_SCOPES_ENV: &str = "TAILSCALE_OAUTH_SCOPES";
pub const OAUTH_JWT_FILE_ENV: &str = "TAILSCALE_OAUTH_JWT_FILE";
pub const TAILNET_ENV: &str = "TAILSCALE_TAILNET";

/// What the API accepts to mean "the tailnet this credential belongs to".
pub const DEFAULT_TAILNET: &str = "-";

/// Every variable this module reads, for the diagnosis subcommand and for the
/// documentation to stay in step with the code.
pub const ENV_VARS: &[&str] = &[
    API_KEY_ENV,
    OAUTH_CLIENT_ID_ENV,
    OAUTH_CLIENT_SECRET_ENV,
    OAUTH_SCOPES_ENV,
    OAUTH_JWT_FILE_ENV,
    TAILNET_ENV,
];

/// How this server proves who it is to the control plane.
#[derive(Debug, Clone)]
pub enum Credentials {
    /// A long-lived API key, sent as a bearer token.
    ApiKey(Secret),
    /// An OAuth client, exchanged for a short-lived access token.
    OauthClient {
        client_id: String,
        client_secret: Secret,
        scopes: Vec<String>,
    },
    /// A workload identity: a JWT written by the platform, exchanged for an
    /// access token. The file is read at exchange time, never cached, because
    /// the platform rotates it underneath us.
    Federated {
        client_id: Option<String>,
        jwt_file: PathBuf,
        scopes: Vec<String>,
    },
}

impl Credentials {
    /// Read a credential from the process environment.
    pub fn from_env() -> Option<Self> {
        Self::from_source(|key| std::env::var(key).ok())
    }

    /// Read a credential from an arbitrary source.
    ///
    /// Setting an environment variable is `unsafe` in this edition and the
    /// workspace forbids `unsafe`, so the tests go through here instead.
    pub fn from_source(source: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let get = |key: &str| {
            source(key)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let scopes = || {
            get(OAUTH_SCOPES_ENV).map_or_else(Vec::new, |raw| {
                raw.split([',', ' '])
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
        };

        if let Some(key) = get(API_KEY_ENV) {
            return Some(Self::ApiKey(Secret::new(key)));
        }
        if let (Some(client_id), Some(secret)) =
            (get(OAUTH_CLIENT_ID_ENV), get(OAUTH_CLIENT_SECRET_ENV))
        {
            return Some(Self::OauthClient {
                client_id,
                client_secret: Secret::new(secret),
                scopes: scopes(),
            });
        }
        if let Some(jwt_file) = get(OAUTH_JWT_FILE_ENV) {
            return Some(Self::Federated {
                client_id: get(OAUTH_CLIENT_ID_ENV),
                jwt_file: PathBuf::from(jwt_file),
                scopes: scopes(),
            });
        }
        None
    }

    /// How this credential is described in diagnostics. Never the value.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::ApiKey(_) => "API key",
            Self::OauthClient { .. } => "OAuth client",
            Self::Federated { .. } => "federated identity",
        }
    }
}

/// The tailnet these credentials act on.
pub fn tailnet_from_env() -> String {
    tailnet_from_source(|key| std::env::var(key).ok())
}

/// The tailnet these credentials act on, from an arbitrary source.
pub fn tailnet_from_source(source: impl Fn(&str) -> Option<String>) -> String {
    source(TAILNET_ENV)
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_TAILNET.to_owned())
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

    #[test]
    fn no_credential_is_a_valid_answer() {
        assert!(Credentials::from_source(env(&[])).is_none());
    }

    #[test]
    fn an_api_key_is_read() {
        let creds = Credentials::from_source(env(&[(API_KEY_ENV, "tskey-api-abc-def")]))
            .expect("a key is a credential");
        match creds {
            Credentials::ApiKey(key) => assert_eq!(key.expose(), "tskey-api-abc-def"),
            other => panic!("expected an API key, got {other:?}"),
        }
    }

    #[test]
    fn an_api_key_wins_over_an_oauth_client() {
        let creds = Credentials::from_source(env(&[
            (API_KEY_ENV, "tskey-api-abc-def"),
            (OAUTH_CLIENT_ID_ENV, "kExAmPlE"),
            (OAUTH_CLIENT_SECRET_ENV, "tskey-client-abc-def"),
        ]))
        .expect("a credential");
        assert_eq!(creds.kind(), "API key");
    }

    #[test]
    fn an_oauth_client_wins_over_a_federated_identity() {
        let creds = Credentials::from_source(env(&[
            (OAUTH_CLIENT_ID_ENV, "kExAmPlE"),
            (OAUTH_CLIENT_SECRET_ENV, "tskey-client-abc-def"),
            (OAUTH_JWT_FILE_ENV, "/run/secrets/token"),
        ]))
        .expect("a credential");
        assert_eq!(creds.kind(), "OAuth client");
    }

    #[test]
    fn a_jwt_file_alone_is_a_federated_identity() {
        let creds = Credentials::from_source(env(&[(OAUTH_JWT_FILE_ENV, "/run/secrets/token")]))
            .expect("a credential");
        match creds {
            Credentials::Federated {
                client_id,
                jwt_file,
                ..
            } => {
                assert_eq!(client_id, None);
                assert_eq!(jwt_file, PathBuf::from("/run/secrets/token"));
            }
            other => panic!("expected a federated identity, got {other:?}"),
        }
    }

    #[test]
    fn half_an_oauth_client_is_not_a_credential() {
        assert!(Credentials::from_source(env(&[(OAUTH_CLIENT_ID_ENV, "kExAmPlE")])).is_none());
        assert!(
            Credentials::from_source(env(&[(OAUTH_CLIENT_SECRET_ENV, "tskey-client-abc")]))
                .is_none()
        );
    }

    #[test]
    fn an_empty_or_blank_value_is_not_a_credential() {
        assert!(Credentials::from_source(env(&[(API_KEY_ENV, "")])).is_none());
        assert!(Credentials::from_source(env(&[(API_KEY_ENV, "   ")])).is_none());
    }

    #[test]
    fn scopes_accept_either_separator() {
        for raw in [
            "devices:read,dns:read",
            "devices:read dns:read",
            " devices:read , dns:read ",
        ] {
            let creds = Credentials::from_source(env(&[
                (OAUTH_CLIENT_ID_ENV, "kExAmPlE"),
                (OAUTH_CLIENT_SECRET_ENV, "tskey-client-abc"),
                (OAUTH_SCOPES_ENV, raw),
            ]))
            .expect("a credential");
            match creds {
                Credentials::OauthClient { scopes, .. } => {
                    assert_eq!(scopes, ["devices:read", "dns:read"], "from {raw:?}");
                }
                other => panic!("expected an OAuth client, got {other:?}"),
            }
        }
    }

    #[test]
    fn the_tailnet_defaults_to_the_one_the_credential_belongs_to() {
        assert_eq!(tailnet_from_source(env(&[])), DEFAULT_TAILNET);
        assert_eq!(
            tailnet_from_source(env(&[(TAILNET_ENV, "  ")])),
            DEFAULT_TAILNET
        );
        assert_eq!(
            tailnet_from_source(env(&[(TAILNET_ENV, "example.com")])),
            "example.com"
        );
    }

    #[test]
    fn a_credential_never_prints_its_value() {
        let creds = Credentials::from_source(env(&[(API_KEY_ENV, "tskey-api-abc-secretpart")]))
            .expect("a credential");
        assert!(!format!("{creds:?}").contains("secretpart"), "{creds:?}");
    }
}
