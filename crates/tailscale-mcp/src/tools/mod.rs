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
pub mod tailnet_dns;
pub mod tailnet_invites;
pub mod tailnet_keys;
pub mod tailnet_logging;
pub mod tailnet_oauth;
pub mod tailnet_org;
pub mod tailnet_policy;
pub mod tailnet_posture;
pub mod tailnet_services;
pub mod tailnet_settings;
pub mod tailnet_users;
pub mod tailnet_webhooks;

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
    all.extend(tailnet_dns::entries());
    all.extend(tailnet_policy::entries());
    all.extend(tailnet_keys::entries());
    all.extend(tailnet_invites::entries());
    all.extend(tailnet_users::entries());
    all.extend(tailnet_settings::entries());
    all.extend(tailnet_posture::entries());
    all.extend(tailnet_webhooks::entries());
    all.extend(tailnet_services::entries());
    all.extend(tailnet_oauth::entries());
    all.extend(tailnet_logging::entries());
    all.extend(tailnet_org::entries());
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
    fn no_tool_schema_carries_a_boolean_items() {
        // `items: true` is what schemars renders for a `serde_json::Value`
        // field. It is valid JSON Schema, but strict tool-schema converters
        // reject the whole request over it — llama.cpp answers
        // `Unrecognized schema: true`, Moonshot answers `items must be an
        // object` — so a single such field takes down every tool call, not
        // just the tool that carries it (issue #1).
        fn walk(node: &serde_json::Value, path: &str, offenders: &mut Vec<String>) {
            if let Some(object) = node.as_object() {
                for (key, value) in object {
                    if key == "items" && value.is_boolean() {
                        offenders.push(format!("{path}/items"));
                    }
                    walk(value, &format!("{path}/{key}"), offenders);
                }
            } else if let Some(array) = node.as_array() {
                for (index, value) in array.iter().enumerate() {
                    walk(value, &format!("{path}[{index}]"), offenders);
                }
            }
        }

        let mut offenders = Vec::new();
        for entry in entries() {
            let name = entry.meta.name;
            let schema = (entry.schema)().expect("a valid schema");
            let schema = serde_json::Value::Object((*schema).clone());
            walk(&schema, name, &mut offenders);
        }
        assert!(offenders.is_empty(), "boolean `items` in: {offenders:?}");
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
