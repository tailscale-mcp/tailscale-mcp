//! The tool table.
//!
//! One module per toolset, each declaring its tools with the `tools!` macro so
//! that the metadata row and the handler come from a single declaration. This
//! module is the only place that knows about all of them, and [`entries`] is
//! what the server is built from.

pub mod common;
pub mod local_files;
pub mod local_prefs;
pub mod local_serve;
pub mod local_status;

/// Every tool this server can offer, before any gating.
pub fn entries() -> Vec<crate::registry::ToolEntry> {
    let mut all = Vec::new();
    all.extend(local_status::entries());
    all.extend(local_prefs::entries());
    all.extend(local_serve::entries());
    all.extend(local_files::entries());
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
