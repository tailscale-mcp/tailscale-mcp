//! `server.json` is what the MCP registry will accept.
//!
//! The listing is a file nothing in this repository reads: it is written for
//! somebody else's validator, and the first time it is checked is when the
//! release tries to publish it. So it is checked here instead, against the
//! registry's own schema — vendored, the way `tailscale-rest` vendors the
//! control-plane description, so the suite stays offline and the answer does
//! not change under us.
//!
//! The vendored copy is pinned by date in its `$id`, and the listing names the
//! same URL in its `$schema`. Those two agreeing is what makes "validated" mean
//! anything: a listing written against a newer schema and checked against an
//! older one has been checked against nothing in particular.
//!
//! Three agreements the schema cannot see are checked here as well, because
//! each is between the listing and something outside it. The registry proves
//! that the packages are ours by looking for its own name for this server in
//! each of them — `mcpName` in the npm package, a label on the image — so
//! those two and the listing have to say the same string or the publish is
//! refused. The variables the listing documents have to be ones this server
//! reads. And the two packages offer the same server, so they offer the same
//! settings.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod repo;
mod schema;

use schema::validate;
use serde_json::Value;

/// The vendored schema, and the listing it judges.
fn schema_and_listing() -> (Value, Value) {
    (
        schema::json_at(
            repo::root()
                .join("packaging")
                .join("registry")
                .join("server.schema.json"),
        ),
        schema::json_at(repo::root().join("server.json")),
    )
}

/// The packages the listing offers.
fn packages(listing: &Value) -> impl Iterator<Item = &Value> {
    listing
        .get("packages")
        .and_then(Value::as_array)
        .expect("the listing offers packages")
        .iter()
}

#[test]
fn the_listing_validates_against_the_registrys_schema() {
    let (schema, listing) = schema_and_listing();
    if let Err(why) = validate(schema, &listing) {
        panic!("the registry would refuse server.json:\n{why}");
    }
}

#[test]
fn the_listing_names_the_schema_it_is_checked_against() {
    let (schema, listing) = schema_and_listing();
    assert_eq!(
        listing["$schema"], schema["$id"],
        "server.json is written against one schema and checked against another"
    );
}

#[test]
fn the_listing_is_at_the_version_being_released() {
    // The registry rejects a listing whose version has already been published,
    // so this is the field a release has to move — and each package carries it
    // again, because a listing can offer versions of itself that differ.
    let (_, listing) = schema_and_listing();
    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(listing["version"], version);
    for package in listing["packages"].as_array().expect("packages") {
        assert_eq!(
            package["version"], version,
            "`{}` is listed at another version",
            package["identifier"]
        );
    }
}

#[test]
fn every_variable_the_listing_documents_is_one_this_server_reads() {
    // The listing is what a client shows somebody configuring this server, so
    // a variable named wrongly here is a setting that silently does nothing.
    let (_, listing) = schema_and_listing();
    let known: std::collections::BTreeSet<&str> = tailscale_mcp::config::ENV_VARS
        .iter()
        .chain(tailscale_rest::credentials::ENV_VARS)
        .copied()
        .collect();
    for package in packages(&listing) {
        for variable in package["environmentVariables"]
            .as_array()
            .expect("environmentVariables")
        {
            let name = variable["name"].as_str().expect("a name");
            assert!(
                known.contains(name),
                "`{}` documents {name}, which this server does not read",
                package["identifier"]
            );
        }
    }
}

#[test]
fn both_packages_offer_the_same_settings() {
    // Two ways to run one server: the same variables do the same things in
    // each, and a setting added to one and not the other is a difference
    // nobody meant.
    let (_, listing) = schema_and_listing();
    let settings: Vec<&Value> = packages(&listing)
        .map(|package| &package["environmentVariables"])
        .collect();
    let [first, second] = settings[..] else {
        panic!("the listing offers {} packages, not two", settings.len());
    };
    assert_eq!(
        first, second,
        "the npm package and the image are configured differently"
    );
}

#[test]
fn everything_the_registry_pulls_claims_this_servers_name() {
    // The registry does not take our word for it that these packages are
    // ours: it fetches each one and looks for its own name for this server
    // inside. A listing whose name has moved and whose packages have not is a
    // publish the registry refuses, which is a thing to find out here.
    let (_, listing) = schema_and_listing();
    let name = listing["name"].as_str().expect("the server's name");

    let package: Value = schema::json_at(
        repo::root()
            .join("packaging")
            .join("npm")
            .join("package.json"),
    );
    assert_eq!(
        package["mcpName"], name,
        "the npm package does not claim this server's name"
    );

    let dockerfile = std::fs::read_to_string(repo::root().join("Dockerfile")).expect("Dockerfile");
    let label = format!("LABEL io.modelcontextprotocol.server.name=\"{name}\"");
    assert!(
        dockerfile.contains(&label),
        "the image does not claim this server's name; expected {label}"
    );
}

#[test]
fn the_image_the_listing_offers_is_the_one_this_release_pushes() {
    // An OCI identifier is `registry/namespace/repository:tag`, and the tag is
    // the release's version: without it the listing offers whatever `:latest`
    // happens to be when somebody reads it.
    let (_, listing) = schema_and_listing();
    let version = env!("CARGO_PKG_VERSION");
    for package in packages(&listing) {
        if package["registryType"] != "oci" {
            continue;
        }
        let identifier = package["identifier"].as_str().expect("an identifier");
        assert!(
            identifier.ends_with(&format!(":{version}")),
            "{identifier} is not the image this release pushes"
        );
    }
}

#[test]
fn the_check_catches_a_listing_the_registry_would_refuse() {
    // Each of these is a way the listing could be wrong that reading it would
    // not catch, so the check is known to fire rather than assumed to.
    let (schema, listing) = schema_and_listing();
    schema::refuses(
        &schema,
        [
            ("a name that is not reverse-DNS with one slash", {
                let mut broken = listing.clone();
                broken["name"] = "tailscale-mcp".into();
                broken
            }),
            ("a description over the hundred characters allowed", {
                let mut broken = listing.clone();
                broken["description"] = "x".repeat(101).into();
                broken
            }),
            ("a package with no transport", {
                let mut broken = listing.clone();
                broken["packages"][0]
                    .as_object_mut()
                    .expect("a package")
                    .remove("transport");
                broken
            }),
            ("no version at all", {
                let mut broken = listing.clone();
                broken
                    .as_object_mut()
                    .expect("the listing")
                    .remove("version");
                broken
            }),
        ],
    );
}
