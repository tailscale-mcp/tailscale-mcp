//! A string that does not print itself.

use std::fmt;

/// A credential.
///
/// The only reason this type exists is its [`fmt::Debug`] implementation:
/// `Config`, `Credentials` and everything holding them derive `Debug`, and a
/// derived `Debug` on a `String` field is how tokens end up in logs.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value itself. Every call site is a place to check.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_never_prints_itself() {
        let secret = Secret::new("tskey-api-kXaMpLe1CNTRL-secretpart");
        assert!(!format!("{secret:?}").contains("secretpart"));
        assert!(!format!("{secret}").contains("secretpart"));
        assert!(!format!("{:?}", Some(secret.clone())).contains("secretpart"));
        assert_eq!(secret.expose(), "tskey-api-kXaMpLe1CNTRL-secretpart");
    }
}
