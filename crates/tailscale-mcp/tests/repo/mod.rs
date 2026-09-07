//! Where the repository is.
//!
//! Some of the checks in this directory are about the repository rather than
//! about a crate — that no fixture carries a real identity, that no workflow
//! needs a credential, that the release is at one version — and each has to
//! walk up out of the crate it is compiled into to find it. This is that walk,
//! in one place.

// Included by every check that reads the repository, and no one of them uses
// every helper here — the same reason `harness/mod.rs` carries this.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// The workspace root.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate is two levels below the workspace root")
        .to_owned()
}

/// Every `.rs` file under a directory, recursively, in a stable order.
///
/// The checks that read the source rather than run it want the whole tree and
/// want it the same way every time, so a failure names the same file twice.
pub fn rust_sources(under: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![under.to_owned()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|kind| kind == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
