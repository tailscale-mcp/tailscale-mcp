//! No parameter reaches a model without a sentence saying what it is.
//!
//! A tool's own description is checked by the doc generator, which would
//! notice an empty cell. Its *parameters* were checked by nothing, and five of
//! them had gone out that way: `tailnet_key_update` carried `issuer`,
//! `subject`, `audience`, `custom_claim_rules` and `description` with no
//! documentation at all, while `KeyCreateParams` — the struct directly above
//! it in the same file, with the same four federated-identity fields —
//! described every one. The update struct was written from the create struct
//! and the doc comments were not carried across.
//!
//! That is worse than a gap in prose. A model choosing arguments has the
//! schema and nothing else: `custom_claim_rules` is a bare
//! `object|null` of strings, and `audience`, `issuer` and `subject` are three
//! same-shaped strings whose whole meaning is which OIDC claim they check.
//!
//! A description may live behind a `$ref` into `$defs` — `tailscale_ip`'s
//! `family` is one — so this resolves those rather than reading the property
//! node alone, which is the reading that once made this look like four bugs
//! it did not have.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use serde_json::Value;

/// Whether a schema node says what it is, following one `$ref` into `$defs`
/// and accepting a description on any branch of a `oneOf`/`anyOf`/`allOf`.
fn describes_itself(defs: Option<&Value>, node: &Value, depth: u8) -> bool {
    if depth > 4 {
        return false;
    }
    if node
        .get("description")
        .and_then(Value::as_str)
        .is_some_and(|d| !d.trim().is_empty())
    {
        return true;
    }
    if let Some(reference) = node.get("$ref").and_then(Value::as_str)
        && let Some(name) = reference.strip_prefix("#/$defs/")
        && let Some(target) = defs.and_then(|defs| defs.get(name))
    {
        return describes_itself(defs, target, depth + 1);
    }
    ["oneOf", "anyOf", "allOf"].iter().any(|key| {
        node.get(key)
            .and_then(Value::as_array)
            .is_some_and(|branches| {
                branches
                    .iter()
                    .any(|branch| describes_itself(defs, branch, depth + 1))
            })
    })
}

#[test]
fn every_parameter_of_every_tool_says_what_it_is() {
    let mut undocumented: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for entry in tailscale_mcp::tools::entries() {
        let schema = (entry.schema)().expect("the schema builds");
        let schema = Value::Object((*schema).clone());
        let defs = schema.get("$defs");
        // A tool with no parameters has no properties. That is the correct
        // schema for it, not a missing one.
        let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
            continue;
        };
        for (name, node) in properties {
            checked += 1;
            if !describes_itself(defs, node, 0) {
                undocumented.push(format!("{}.{name}", entry.meta.name));
            }
        }
    }
    assert!(
        checked > 100,
        "only {checked} parameters were examined, which is too few to have \
         walked the real table"
    );
    assert!(
        undocumented.is_empty(),
        "{} parameters reach a model with nothing saying what they are: {}",
        undocumented.len(),
        undocumented.join(", ")
    );
}

/// And the check can tell a described parameter from an undescribed one.
#[test]
fn the_check_reads_a_description_wherever_it_is_written() {
    let defs = serde_json::json!({
        "AddressFamily": {"description": "Which addresses to report."}
    });
    let defs = Some(&defs);

    assert!(describes_itself(
        defs,
        &serde_json::json!({"description": "Plainly."}),
        0
    ));
    assert!(
        describes_itself(
            defs,
            &serde_json::json!({"$ref": "#/$defs/AddressFamily"}),
            0
        ),
        "a bare $ref into $defs, which is how `tailscale_ip.family` is written"
    );
    assert!(describes_itself(
        defs,
        &serde_json::json!({"oneOf": [{"type": "null"}, {"description": "One branch."}]}),
        0
    ));

    assert!(!describes_itself(
        defs,
        &serde_json::json!({"type": "string"}),
        0
    ));
    assert!(
        !describes_itself(defs, &serde_json::json!({"description": "  "}), 0),
        "whitespace is not a description"
    );
    assert!(
        !describes_itself(defs, &serde_json::json!({"$ref": "#/$defs/Absent"}), 0),
        "a $ref to something that is not there describes nothing"
    );
}
