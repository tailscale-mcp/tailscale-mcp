//! Judging a document against a vendored JSON Schema.
//!
//! Two files here are written for somebody else's validator and read by
//! nothing in this repository: the registry listing and the bundle manifest.
//! Each is checked against a vendored copy of the schema that will judge it,
//! the way `tailscale-rest` vendors the control-plane description, so the
//! suite stays offline and the answer does not change under us. The compiling
//! is the same both times, so it lives here.

use boon::{Compiler, Schemas};
use serde_json::Value;

/// Judge one document against a schema, saying why if it is refused.
///
/// The schema is handed over whole, so nothing is fetched; `boon` addresses
/// schemas by URL all the same, and a name that cannot resolve is the honest
/// one to give a document that came from disk.
pub fn validate(schema: Value, instance: &Value) -> Result<(), String> {
    const HERE: &str = "https://tailscale-mcp.invalid/vendored.schema.json";
    let mut schemas = Schemas::new();
    let mut compiler = Compiler::new();
    compiler
        .add_resource(HERE, schema)
        .expect("the vendored schema compiles");
    let compiled = compiler
        .compile(HERE, &mut schemas)
        .expect("the vendored schema compiles");
    schemas
        .validate(instance, compiled)
        .map_err(|error| format!("{error:#}"))
}

/// Read a JSON file, or say which one was not there.
pub fn json_at(path: std::path::PathBuf) -> Value {
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|why| panic!("{}: {why}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|why| panic!("{} is not JSON: {why}", path.display()))
}

/// Check that a schema refuses each of a set of broken documents.
///
/// Both callers need this and for the same reason: a validator that accepts
/// everything passes just as quietly as one that works, so each check is shown
/// a document it must refuse. `cases` is `(what is wrong with it, the
/// document)`, and the panic names the one that got through.
pub fn refuses(schema: &Value, cases: impl IntoIterator<Item = (&'static str, Value)>) {
    for (what, broken) in cases {
        assert!(
            validate(schema.clone(), &broken).is_err(),
            "{what} should have been caught"
        );
    }
}
