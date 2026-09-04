//! The policy file, and what the control plane says about one.
//!
//! The policy itself is HuJSON — comments and trailing commas — so it is not a
//! model: it travels as text, under `application/hujson`, and comes back as
//! whatever the caller's `Accept` asked for. What is modelled here is
//! everything *around* it — the answers `acl/preview` and `acl/validate` give,
//! and the test case `acl/validate` takes — because those are ordinary JSON
//! whatever format the document itself is in.

use serde_json::Value;

use crate::model;
use crate::models::KnownValues;

/// What a policy preview can be asked about.
///
/// `user` previews the rules that would match a user, `ipport` the rules that
/// would match an address and port. The parameter is called `type`, which is
/// why the constant is not.
pub const PREVIEW_SUBJECTS: &[&str] = &["user", "ipport"];

pub const KNOWN_VALUES: &[KnownValues] =
    &[("/tailnet/{tailnet}/acl/preview ?type", PREVIEW_SUBJECTS)];

model! {
    /// What previewing a policy answers: the rules that would match, and the
    /// question echoed back.
    PolicyPreview as "POST /tailnet/{tailnet}/acl/preview 200" {
        matches: "matches" => Vec<PolicyMatch>,
        /// Echoes the `type` asked for.
        subject_type: "type" => String,
        /// Echoes the `previewFor` asked for.
        preview_for: "previewFor" => String,
    }

    /// One rule that would match, and where in the document it is written.
    PolicyMatch as "POST /tailnet/{tailnet}/acl/preview 200.matches[]" {
        /// The sources the rule affects.
        users: "users" => Vec<String>,
        /// The destinations it reaches.
        ports: "ports" => Vec<String>,
        /// Which line of the policy file the rule is on, so that a caller can
        /// go and read it.
        line_number: "lineNumber" => i64,
    }

    /// What validating a policy answers *when something is wrong*.
    ///
    /// A pass is an empty body, so a caller that receives any of this has a
    /// failure or a warning to read. `data` is left as [`Value`] because its
    /// items differ per finding — a failed test carries `errors`, an
    /// unsynced group carries `warnings` — and the description gives them no
    /// properties at all.
    PolicyValidation as "POST /tailnet/{tailnet}/acl/validate 200" {
        /// `test(s) failed`, `warning(s) found`, and the like.
        message: "message" => String,
        /// One entry per finding, in the control plane's own shape.
        data: "data" => Vec<Value>,
    }

    /// One test case, as `acl/validate` takes them.
    ///
    /// Modelled although it is only ever sent, because a caller writes these
    /// by hand and a name the description does not have is a test that
    /// silently does not run.
    PolicyTest as "POST /tailnet/{tailnet}/acl/validate body (application/json)|oneOf[0][]" {
        /// The identity the test runs as: an email address, a group, a tag or
        /// a host.
        src: "src" => String,
        /// Posture attributes to evaluate posture conditions against, as
        /// `{"node:os": "windows"}`. Only needed by a policy that has them.
        src_posture_attrs: "srcPostureAttrs" => std::collections::BTreeMap<String, Value>,
        /// `tcp`, `udp` and the rest. Omitted tests either.
        proto: "proto" => String,
        /// `host:port` destinations this identity must reach.
        accept: "accept" => Vec<String>,
        /// `host:port` destinations it must not.
        deny: "deny" => Vec<String>,
    }
}
