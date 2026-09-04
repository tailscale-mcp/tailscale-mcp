//! A tripwire on a refresh of the vendored API description.
//!
//! `spec.md` names this as one of two places behaviour is invisible from
//! above, and says what it is for: "Schema drift is tested by parsing the
//! vendored API description and asserting every property is modelled. This is
//! not a behavioural test; it is a tripwire on a refresh."
//!
//! So nothing here calls the control plane or exercises a model. It reads
//! `docs/research/tailscale-openapi.yaml`, walks every object it describes,
//! and holds `tailscale_rest::models` to it in both directions: a property the
//! description has and a model does not is a field this build would drop, and
//! a model the description no longer has is a shape that has quietly stopped
//! being true. The same in both directions for the documented strings (Q60),
//! whose known values are the other thing a refresh moves.
//!
//! When it fails, the failure names the schema and the property. Adding the
//! field to the model beside its neighbours is usually the whole fix.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

/// The description, relative to this crate.
const DESCRIPTION: &str = "../../docs/research/tailscale-openapi.yaml";

/// Every object the description describes: the path it is reached by, and the
/// JSON names it carries.
type Objects = BTreeMap<String, BTreeSet<String>>;

/// Every documented string, and the values the description gives it.
type Enums = BTreeMap<String, Vec<String>>;

fn description() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DESCRIPTION);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the vendored description is at {}: {e}", path.display()));
    // Parsed rather than scanned (Q61): a reader that misreads a block sees
    // fewer properties than are there, and a drift test that under-reads its
    // input passes, which is the one failure this file exists to prevent.
    serde_norway::from_str(&text).expect("the vendored description is YAML")
}

fn schemas(document: &Value) -> &Map<String, Value> {
    document["components"]["schemas"]
        .as_object()
        .expect("the description has component schemas")
}

/// Everything the description says, by path.
///
/// A named schema is reached by its name; an object inside one by the path that
/// gets to it — `Device.clientConnectivity`, `Webhook.subscriptions[]` for an
/// array's items, `Device.clientConnectivity.latency{}` for a map's values.
///
/// `components/schemas` is not the whole document, which is the thing this walk
/// got wrong first. Forty-six more objects and ten more enumerations are inline
/// in `paths` and in `components/parameters`, and a walk that started at the
/// named schemas could not see any of them (Q64). So request bodies, responses
/// and parameters are walked too, under paths that say where they came from:
/// `POST /keys body`, `GET /tailnet/{tailnet}/devices 200`, `?fields`.
fn read(document: &Value) -> (Objects, Enums) {
    let mut objects = Objects::new();
    let mut enums = Enums::new();
    let all = schemas(document);
    let mut record = |node: &Value, at: String| {
        walk_unless_ref(node, &at, all, &mut objects, &mut enums);
    };

    for (name, schema) in all {
        record(schema, name.clone());
    }
    for parameter in shared_parameters(document).values() {
        if let Some((at, schema)) = parameter_schema(parameter, "") {
            record(schema, at);
        }
    }
    for (route, operations) in paths(document) {
        let Some(operations) = operations.as_object() else {
            continue;
        };
        for (verb, operation) in operations {
            // A `parameters` list sits beside the verbs as well as inside
            // them; beside them it applies to every verb on the route, so it
            // belongs to the route rather than to one call on it.
            if verb == "parameters" {
                for parameter in operation.as_array().into_iter().flatten() {
                    if let Some((at, schema)) = parameter_schema(parameter, &route) {
                        record(schema, at);
                    }
                }
                continue;
            }
            let prefix = format!("{} {route}", verb.to_uppercase());
            for parameter in operation["parameters"].as_array().into_iter().flatten() {
                if let Some((at, schema)) = parameter_schema(parameter, &prefix) {
                    record(schema, at);
                }
            }
            for (at, schema) in body_schemas(&operation["requestBody"], &format!("{prefix} body")) {
                record(schema, at);
            }
            for (code, response) in operation["responses"].as_object().into_iter().flatten() {
                for (at, schema) in body_schemas(response, &format!("{prefix} {code}")) {
                    record(schema, at);
                }
            }
        }
    }
    (objects, enums)
}

fn paths(document: &Value) -> Vec<(String, Value)> {
    document["paths"]
        .as_object()
        .expect("the description has paths")
        .iter()
        .map(|(route, operations)| (route.clone(), operations.clone()))
        .collect()
}

fn shared_parameters(document: &Value) -> &Map<String, Value> {
    document["components"]["parameters"]
        .as_object()
        .expect("the description has shared parameters")
}

/// Every schema under a request body or a response, one per media type.
///
/// Every one, not the first one, which is the second thing this walk got wrong.
/// The policy endpoints carry the same body twice — `application/json` holding
/// the real shape and `application/hujson` holding a bare string — and reading
/// whichever the map happened to yield first meant reading the string and never
/// seeing the object beside it, so a five-property test case went unmodelled
/// while the test stayed green (Q71).
///
/// One media type keeps the plain path, because that is nearly every body here
/// and a suffix on all of them would say nothing. Where there are several, each
/// is named, so the path still says exactly which schema failed. Endpoints that
/// answer with no body have no content and yield nothing.
fn body_schemas<'a>(carrier: &'a Value, at: &str) -> Vec<(String, &'a Value)> {
    let Some(content) = carrier.get("content").and_then(Value::as_object) else {
        return Vec::new();
    };
    let carrying: Vec<_> = content
        .iter()
        .filter_map(|(media, body)| Some((media, body.get("schema")?)))
        .collect();
    let name = |media: &String| match carrying.len() {
        1 => at.to_owned(),
        _ => format!("{at} ({media})"),
    };
    carrying
        .iter()
        .map(|(media, schema)| (name(media), *schema))
        .collect()
}

/// Where a parameter's schema is, and what to call it.
///
/// `?fields` for one the whole document shares, `GET /tailnet/{tailnet}/users
/// ?role` for one belonging to a single call. A `$ref` here points into
/// `components/parameters`, which is walked whole, so following it would only
/// record the same enumeration twice under a longer name.
fn parameter_schema<'a>(parameter: &'a Value, prefix: &str) -> Option<(String, &'a Value)> {
    if parameter.get("$ref").is_some() {
        return None;
    }
    let name = parameter.get("name")?.as_str()?;
    let schema = parameter.get("schema")?;
    let at = if prefix.is_empty() {
        format!("?{name}")
    } else {
        format!("{prefix} ?{name}")
    };
    Some((at, schema))
}

/// Walk it, unless it is a `$ref` — the target is walked at its own name, and
/// following it here would record the same object twice under two paths.
fn walk_unless_ref(
    node: &Value,
    path: &str,
    all: &Map<String, Value>,
    objects: &mut Objects,
    enums: &mut Enums,
) {
    if node.get("$ref").is_some() {
        return;
    }
    walk(node, path, all, objects, enums);
}

fn walk(
    node: &Value,
    path: &str,
    all: &Map<String, Value>,
    objects: &mut Objects,
    enums: &mut Enums,
) {
    if let Some(values) = node.get("enum").and_then(Value::as_array) {
        let values = values
            .iter()
            .map(|v| {
                v.as_str()
                    .unwrap_or_else(|| panic!("{path} enumerates something that is not a string"))
                    .to_owned()
            })
            .collect();
        enums.insert(path.to_owned(), values);
    }

    let properties = node.get("properties").and_then(Value::as_object);
    let mut names: BTreeSet<String> = properties.map(named).unwrap_or_default();

    // `allOf` is one object described in pieces, so its members' properties are
    // this object's. A `$ref` member contributes the names it points at and
    // nothing more: the target is an object in its own right and is walked at
    // its own path, so following it here would check it twice and record its
    // children under the wrong name.
    for member in node
        .get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(target) = member.get("$ref").and_then(Value::as_str) {
            let target = resolve(target, all);
            assert!(
                target.get("allOf").is_none(),
                "{path} composes {target:?}, which is itself composed; \
                 this walk reads one level and would drop the rest"
            );
            names.extend(
                target
                    .get("properties")
                    .and_then(Value::as_object)
                    .map(named)
                    .unwrap_or_default(),
            );
        } else {
            names.extend(
                member
                    .get("properties")
                    .and_then(Value::as_object)
                    .map(named)
                    .unwrap_or_default(),
            );
            walk(member, path, all, objects, enums);
        }
    }

    // `anyOf` and `oneOf` say a value may be one of several things. Most
    // branches in the description are bare scalars — "a string or a number or
    // a boolean" — and record nothing, having neither properties nor an
    // enumeration. Some are not: the bulk device-attributes body accepts a
    // scalar or a `{value, expiry}` object in the same position. So every
    // branch is walked at a path that names it, and a branch with a shape
    // becomes an object needing a model like any other rather than something
    // the walk steps over, which is the failure mode this file exists for.
    for union in ["anyOf", "oneOf"] {
        for (which, branch) in node
            .get(union)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            walk_unless_ref(
                branch,
                &format!("{path}|{union}[{which}]"),
                all,
                objects,
                enums,
            );
        }
    }

    if !names.is_empty() {
        objects.insert(path.to_owned(), names);
    }

    for (name, child) in properties.into_iter().flatten() {
        walk(child, &format!("{path}.{name}"), all, objects, enums);
    }
    if let Some(items) = node.get("items") {
        walk(items, &format!("{path}[]"), all, objects, enums);
    }
    if let Some(values) = node.get("additionalProperties").filter(|v| v.is_object()) {
        walk(values, &format!("{path}{{}}"), all, objects, enums);
    }
}

fn named(properties: &Map<String, Value>) -> BTreeSet<String> {
    properties.keys().cloned().collect()
}

fn resolve<'a>(reference: &str, all: &'a Map<String, Value>) -> &'a Value {
    let name = reference
        .strip_prefix("#/components/schemas/")
        .unwrap_or_else(|| panic!("{reference} points outside the component schemas"));
    all.get(name)
        .unwrap_or_else(|| panic!("{reference} points at a schema that is not there"))
}

/// The objects the description carries that no model covers yet, and why.
///
/// Every one is a request body a tool builds from its own parameters, or the
/// envelope a listing arrives in — shapes belonging to the tools that send and
/// receive them rather than to this crate's schema layer, and so to the ticket
/// that builds those tools (Q64).
///
/// This is a deferral, not an exemption. A path here must still be in the
/// document, and an object the document grows that is neither modelled nor
/// listed here fails the walk. So the list can only shrink, and it shrinks by
/// a ticket landing rather than by anyone editing this table.
///
/// It is now empty: tickets 17 to 20 each took their rows, and every object the
/// description carries has a model. The table stays because the next refresh of
/// the description is what it is for — an object the document grows has to be
/// modelled or listed here, and an empty list makes the second option visible
/// rather than forgotten.
const DEFERRED: &[(&str, &str)] = &[
    // Ticket 17 — devices and posture.
    // Ticket 18 — DNS and policy: modelled, and the rows gone.
    // Ticket 19 — keys, users, invites and contacts: modelled, and the rows
    // gone.
    // Ticket 20 — webhooks, services and logging: modelled, and the rows gone.
];

fn deferred(path: &str) -> Option<&'static str> {
    DEFERRED
        .iter()
        .find(|(at, _)| *at == path)
        .map(|(_, why)| *why)
}

/// What the models say, in the same shape the description was read into.
fn models() -> Objects {
    tailscale_rest::models::shapes()
        .map(|shape| {
            (
                shape.schema.to_owned(),
                shape.fields.iter().map(|f| (*f).to_owned()).collect(),
            )
        })
        .collect()
}

fn known() -> Enums {
    tailscale_rest::models::known_values()
        .map(|(path, values)| {
            (
                (*path).to_owned(),
                values.iter().map(|v| (*v).to_owned()).collect(),
            )
        })
        .collect()
}

/// Every way the two disagree, as sentences a reader can act on.
///
/// Split out from the tests below so that the tripwire can itself be tripped:
/// a test cannot delete a field from a real model, but it can hand this
/// function a model list with the field taken out and check that it says so.
fn differences(description: &Objects, models: &Objects) -> Vec<String> {
    let mut found = Vec::new();
    for (schema, properties) in description {
        let Some(modelled) = models.get(schema) else {
            if deferred(schema).is_none() {
                found.push(format!(
                    "{schema} is described and has no model; its properties are {}",
                    list(properties)
                ));
            }
            continue;
        };
        let missing = list(&properties.difference(modelled).cloned().collect());
        if !missing.is_empty() {
            found.push(format!("{schema} is missing {missing}"));
        }
        let extra = list(&modelled.difference(properties).cloned().collect());
        if !extra.is_empty() {
            found.push(format!(
                "{schema} models {extra}, which the description does not have"
            ));
        }
    }
    for schema in models.keys() {
        if !description.contains_key(schema) {
            found.push(format!(
                "{schema} is modelled and the description no longer describes it"
            ));
        }
    }
    found
}

/// The same, for the documented strings.
fn value_differences(description: &Enums, known: &Enums) -> Vec<String> {
    let mut found = Vec::new();
    for (path, values) in description {
        match known.get(path) {
            None => found.push(format!(
                "{path} enumerates {} and no constant names them",
                values.join(", ")
            )),
            Some(named) if named != values => found.push(format!(
                "{path} enumerates [{}] and its constant says [{}]",
                values.join(", "),
                named.join(", ")
            )),
            Some(_) => {}
        }
    }
    for path in known.keys() {
        if !description.contains_key(path) {
            found.push(format!(
                "{path} has a constant and the description enumerates nothing there"
            ));
        }
    }
    found
}

fn list(names: &BTreeSet<String>) -> String {
    names.iter().cloned().collect::<Vec<_>>().join(", ")
}

#[test]
fn every_property_the_description_has_is_modelled() {
    let (described, _) = read(&description());
    let found = differences(&described, &models());
    assert!(
        found.is_empty(),
        "the models and the vendored description disagree:\n  {}",
        found.join("\n  ")
    );
}

#[test]
fn every_documented_string_carries_the_values_the_description_gives_it() {
    let (_, described) = read(&description());
    let found = value_differences(&described, &known());
    assert!(
        found.is_empty(),
        "the known values and the vendored description disagree:\n  {}",
        found.join("\n  ")
    );
}

#[test]
fn a_property_dropped_from_a_model_is_a_failure() {
    // The criterion the ticket states, and the only one that cannot be shown
    // by the test above passing: a green drift test proves nothing unless a
    // red one is reachable. The three ways a refresh can move are all here.
    let (described, _) = read(&description());

    let mut short = models();
    short
        .get_mut("Device")
        .expect("Device is modelled")
        .remove("nodeId");
    assert_eq!(
        differences(&described, &short),
        ["Device is missing nodeId"],
        "a field taken off a model has to be noticed"
    );

    let mut gone = models();
    gone.remove("Key");
    assert_eq!(
        differences(&described, &gone),
        [format!(
            "Key is described and has no model; its properties are {}",
            list(described.get("Key").expect("Key is described"))
        )],
        "a whole model going missing has to be noticed"
    );

    let mut invented = models();
    invented
        .get_mut("Contact")
        .expect("Contact is modelled")
        .insert("postalAddress".to_owned());
    assert_eq!(
        differences(&described, &invented),
        ["Contact models postalAddress, which the description does not have"],
        "a field the description dropped has to be noticed too"
    );

    let mut stale = models();
    stale.insert("Fax".to_owned(), BTreeSet::new());
    assert_eq!(
        differences(&described, &stale),
        ["Fax is modelled and the description no longer describes it"]
    );
}

#[test]
fn a_value_the_description_adds_is_a_failure() {
    let (_, described) = read(&description());

    let mut short = known();
    short
        .get_mut("Key.keyType")
        .expect("key types are named")
        .pop();
    assert_eq!(
        value_differences(&described, &short),
        [
            "Key.keyType enumerates [auth, client, api, federated] and its constant says [auth, client, api]"
        ],
        "a value the description gained has to be noticed"
    );

    let mut gone = known();
    gone.remove("LogType");
    assert_eq!(
        value_differences(&described, &gone),
        ["LogType enumerates configuration, network and no constant names them"],
        "a whole enumeration going unnamed has to be noticed"
    );
}

#[test]
fn a_body_carrying_two_media_types_is_read_at_both() {
    // The walk's second under-read (Q71). The policy endpoints describe the
    // same body twice — the real shape under `application/json` and a bare
    // string under `application/hujson` — and reading whichever the map
    // yielded first meant reading the string, so `acl/validate`'s test case
    // was invisible while the test stayed green.
    let document = description();
    let validate = &document["paths"]["/tailnet/{tailnet}/acl/validate"]["post"];
    let found = body_schemas(&validate["requestBody"], "body");
    assert_eq!(found.len(), 2, "both media types carry a schema: {found:?}");

    let (objects, _) = read(&document);
    let test_case = objects
        .get("POST /tailnet/{tailnet}/acl/validate body (application/json)|oneOf[0][]")
        .expect("the test case the JSON branch describes");
    assert!(
        test_case.contains("srcPostureAttrs"),
        "and it is read whole: {test_case:?}"
    );

    // One media type keeps the plain path, so the widening did not rename
    // every body in the document.
    let keys = &document["paths"]["/tailnet/{tailnet}/keys"]["post"];
    assert_eq!(
        body_schemas(&keys["requestBody"], "body")
            .into_iter()
            .map(|(at, _)| at)
            .collect::<Vec<_>>(),
        ["body"]
    );
}

/// Where the document is known to be wrong, or to disagree with itself.
///
/// Each is asserted rather than written down somewhere and left to rot: a
/// refresh that settles one fails the test excusing it, and the note comes out
/// with the code it was there for. Three kinds are mixed here because the fix
/// is the same for all three — the document against itself, the document
/// against Tailscale's own Go client (`docs/research/control-plane-api.md` §7,
/// which is the closest thing to a view of the live API), and the document
/// against what this crate had to do anyway.
mod known_divergences {
    use super::{description, schemas};
    use serde_json::Value;

    #[test]
    fn the_endpoint_this_crate_gets_its_tokens_from_is_not_described() {
        // `token.rs` writes `/api/v2/oauth/token` from Tailscale's own
        // documentation because the description does not have it (ADR-0002).
        // The day it does, the path should come from here instead.
        let document = description();
        let paths = document["paths"]
            .as_object()
            .expect("the description has paths");
        let oauth: Vec<_> = paths.keys().filter(|p| p.contains("oauth/token")).collect();
        assert!(
            oauth.is_empty(),
            "the description now describes {oauth:?}; take the hand-written path out of token.rs"
        );
    }

    #[test]
    fn the_https_setting_is_called_two_things() {
        // The setting is `httpsEnabled` in the schema and `httpsCertificates`
        // in the prose describing the scope that guards it. The model follows
        // the schema, which is what the wire carries.
        let document = description();
        let settings = &schemas(&document)["TailnetSettings"]["properties"];
        assert!(settings.get("httpsEnabled").is_some());
        assert!(
            settings.get("httpsCertificates").is_none(),
            "the schema now has the name the prose uses; the models can follow it"
        );
        let text = serde_json::to_string(&document).expect("the document re-serialises");
        assert!(
            text.contains("httpsCertificates"),
            "the prose no longer disagrees with the schema; this note can go"
        );
    }

    #[test]
    fn split_dns_is_two_different_shapes() {
        // The standalone `SplitDns` maps a domain to plain addresses; the same
        // idea inside `DnsConfiguration` maps it to resolver objects. Both
        // endpoints are served, so `dns.rs` models both rather than picking.
        let document = description();
        let all = schemas(&document);
        let standalone = &all["SplitDns"]["additionalProperties"]["items"];
        assert_eq!(
            standalone.get("type").and_then(Value::as_str),
            Some("string"),
            "the older split-DNS shape has stopped taking bare addresses"
        );
        let nested =
            &all["DnsConfiguration"]["properties"]["splitDNS"]["additionalProperties"]["items"];
        assert_eq!(
            nested.get("$ref").and_then(Value::as_str),
            Some("#/components/schemas/DnsConfigurationResolver"),
            "the two split-DNS shapes have converged; `dns.rs` need only model one"
        );
    }

    #[test]
    fn four_log_stream_fields_cannot_be_reached() {
        // The sharpest of the Go-client divergences (§7): the description
        // gives `LogstreamEndpointConfiguration` four `gcs*` fields and then
        // leaves `gcs` out of the destinations its `destinationType` accepts,
        // so nothing can select the destination those fields configure. The Go
        // client has the value. The fields are modelled because the document
        // has them; this is the note saying why they look unusable.
        let document = description();
        let configuration = &schemas(&document)["LogstreamEndpointConfiguration"]["properties"];
        let gcs: Vec<_> = configuration
            .as_object()
            .expect("it has properties")
            .keys()
            .filter(|name| name.starts_with("gcs"))
            .collect();
        assert_eq!(gcs.len(), 4, "the gcs fields: {gcs:?}");
        assert!(
            !destinations().contains(&"gcs"),
            "the description now offers `gcs`; the four fields are reachable and this note can go"
        );
        assert!(
            destinations().contains(&"crowdstrike"),
            "the description has dropped `crowdstrike`, which the Go client never had"
        );
    }

    fn destinations() -> Vec<&'static str> {
        tailscale_rest::models::logging::DESTINATION_TYPES.to_vec()
    }

    #[test]
    fn the_posture_providers_are_behind_the_go_client() {
        // §7: the Go client knows `fleet` and `huntress`. Q60 is the reason
        // this is a note rather than a bug — the values are a documented
        // string, so a tool passes either through today and only the parameter
        // description is behind.
        let known = tailscale_rest::models::device::POSTURE_PROVIDERS;
        for later in ["fleet", "huntress"] {
            assert!(
                !known.contains(&later),
                "the description has caught up on {later}; drop it from this note"
            );
        }
    }

    #[test]
    fn the_services_endpoint_is_spelled_one_way_here_and_another_in_the_go_client() {
        // §7 and §8 #11: the Go client calls `/vip-services`, the description
        // documents `/services`, and the models are named for the schemas —
        // `VIPServiceInfo` — which follow the Go spelling. Both may be live.
        // The description is authoritative for the path this crate sends.
        let document = description();
        let routes: Vec<_> = document["paths"]
            .as_object()
            .expect("the description has paths")
            .keys()
            .filter(|route| route.contains("services"))
            .collect();
        assert!(
            routes.iter().any(|route| route.ends_with("/services")),
            "the description no longer documents `/services`: {routes:?}"
        );
        assert!(
            !routes.iter().any(|route| route.contains("vip-services")),
            "the description now documents the Go client's spelling too: {routes:?}"
        );
        assert!(
            !schemas(&document)["VIPServiceInfo"]["properties"]
                .as_object()
                .expect("it has properties")
                .contains_key("annotations"),
            "the description has gained the Go client's `annotations`; model it"
        );
    }

    #[test]
    fn a_key_listing_requires_a_parameter_it_calls_optional() {
        // §8 #6, which matters at the moment ticket 19 writes the parameter.
        // `all` is marked required and then described as "If set to true …",
        // which is how an optional parameter is written. Believing the mark
        // would make a caller send `all` on every listing; believing the prose
        // would send none and get a partial list. The prose is the one that
        // describes behaviour, so the parameter is modelled as optional.
        let document = description();
        let listing = &document["paths"]["/tailnet/{tailnet}/keys"]["get"];
        let all = listing["parameters"]
            .as_array()
            .expect("the listing takes parameters")
            .iter()
            .find_map(|parameter| {
                let reference = parameter.get("$ref")?.as_str()?;
                reference.ends_with("/all").then_some(reference)
            })
            .expect("one of them is the shared `all`");
        let shared = &document["components"]["parameters"]["all"];
        assert_eq!(
            shared["required"],
            Value::Bool(true),
            "{all} has stopped being required; the note can go"
        );
        assert!(
            shared["description"]
                .as_str()
                .expect("it is described")
                .starts_with("If set to true"),
            "a required parameter has stopped being described as one that may be unset"
        );
    }
}

/// What the walk finds, pinned.
///
/// Not a check on the models — the tests above are that — but on the walk
/// itself. A refresh that changes the document's *structure* rather than its
/// contents shows up here as a jump, instead of as a walk that quietly finds
/// less and passes.
#[test]
fn the_walk_reaches_the_whole_document() {
    let document = description();
    let (objects, enums) = read(&document);
    assert_eq!(schemas(&document).len(), 43, "named schemas");
    assert_eq!(objects.len(), 91, "objects walked: {:?}", objects.keys());
    assert_eq!(enums.len(), 33, "enumerations walked: {:?}", enums.keys());
    assert_eq!(models().len(), 91, "models");
    assert_eq!(DEFERRED.len(), 0, "deferrals");
}

/// The deferral table cannot outlive what it defers.
///
/// A row for a path the description has dropped would be a model nobody owes
/// any more, sitting in a list that reads as work outstanding. And a row that
/// duplicates a model would exempt a shape that is in fact checked.
#[test]
fn every_deferral_names_something_the_description_still_has() {
    let (described, _) = read(&description());
    let models = models();
    for (path, why) in DEFERRED {
        assert!(
            described.contains_key(*path),
            "{path} is deferred ({why}) and the description no longer has it"
        );
        assert!(
            !models.contains_key(*path),
            "{path} is deferred ({why}) and is also modelled; the row can go"
        );
    }
}
