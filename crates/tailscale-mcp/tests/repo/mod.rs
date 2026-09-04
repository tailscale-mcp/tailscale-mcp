//! Where the repository is.
//!
//! Some of the checks in this directory are about the repository rather than
//! about a crate — that no fixture carries a real identity, that no workflow
//! needs a credential, that the release is at one version — and each has to
//! walk up out of the crate it is compiled into to find it. This is that walk,
//! in one place.

use std::path::{Path, PathBuf};

/// The workspace root.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate is two levels below the workspace root")
        .to_owned()
}
