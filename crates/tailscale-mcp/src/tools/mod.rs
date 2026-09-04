//! The tool table.
//!
//! One module per toolset, each declaring its tools with the `tools!` macro so
//! that the metadata row and the handler come from a single declaration. This
//! module is the only place that knows about all of them, and [`entries`] is
//! what the server is built from.

pub mod common;
pub mod local_debug;
pub mod local_files;
pub mod local_lock;
pub mod local_prefs;
pub mod local_serve;
pub mod local_status;
pub mod passthrough;
pub mod tailnet_devices;
pub mod tailnet_posture;

/// Every tool this server can offer, before any gating.
pub fn entries() -> Vec<crate::registry::ToolEntry> {
    let mut all = Vec::new();
    all.extend(local_status::entries());
    all.extend(local_prefs::entries());
    all.extend(local_serve::entries());
    all.extend(local_files::entries());
    all.extend(local_lock::entries());
    all.extend(local_debug::entries());
    all.extend(passthrough::entries());
    all.extend(tailnet_devices::entries());
    all.extend(tailnet_posture::entries());
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;

    #[test]
    fn the_whole_table_forms_a_valid_registry() {
        // Names, duplicates, schemas and the confirmation rule are all checked
        // by `Registry::new`, so this one assertion covers the whole table.
        Registry::new(entries()).expect("the tool table is valid");
    }

    #[test]
    fn every_tailnet_tool_ends_in_a_known_verb() {
        // `spec.md`: tailnet tools are named `tailnet_<resource>_<verb>` "with
        // a fixed verb vocabulary". Fixed means this list, and means a name
        // that does not fit is a name to reconsider rather than a word to add.
        use crate::meta::{Surface, TAILNET_VERBS};

        for entry in entries() {
            let name = entry.meta.name;
            if entry.meta.surface() != Surface::Tailnet {
                continue;
            }
            let verb = name.rsplit('_').next().expect("a name has a last word");
            assert!(
                TAILNET_VERBS.contains(&verb),
                "`{name}` ends in `{verb}`, which is not one of {TAILNET_VERBS:?}"
            );
            assert!(
                name.matches('_').count() >= 2,
                "`{name}` needs a resource between the prefix and the verb"
            );
        }
    }

    #[test]
    fn every_tool_carries_its_surface_prefix() {
        for entry in entries() {
            assert!(
                entry.meta.name.starts_with(entry.meta.surface().prefix()),
                "`{}` belongs to the {} surface",
                entry.meta.name,
                entry.meta.surface()
            );
        }
    }
}
