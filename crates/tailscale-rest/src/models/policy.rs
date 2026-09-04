//! The policy file, and what the control plane says about one.
//!
//! The policy itself is HuJSON — comments and trailing commas — and travels as
//! text rather than as a model, so there is no struct here yet. Ticket 18 adds
//! the shapes `acl/preview` and `acl/validate` answer with; what this module
//! holds today is the one documented string those endpoints take.

use crate::models::KnownValues;

/// What a policy preview can be asked about.
///
/// `user` previews the rules that would match a user, `ipport` the rules that
/// would match an address and port. The parameter is called `type`, which is
/// why the constant is not.
pub const PREVIEW_SUBJECTS: &[&str] = &["user", "ipport"];

pub const KNOWN_VALUES: &[KnownValues] =
    &[("/tailnet/{tailnet}/acl/preview ?type", PREVIEW_SUBJECTS)];

/// No models yet; ticket 18 fills this in.
pub const SHAPES: &[crate::models::ModelShape] = &[];
