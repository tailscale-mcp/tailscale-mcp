//! The shapes the control plane sends, written by hand (ADR-0003).
//!
//! Every model here is a struct of optional fields plus a map of everything
//! the description did not mention, so a control plane that grows a field
//! parses today and the new field is readable without a release. Nothing is
//! renamed on the way through: ADR-0004 has Tailscale's bodies come back in
//! Tailscale's shape, so the JSON names are Tailscale's and the Rust names
//! exist only to be spelled in Rust.
//!
//! Enums are documented strings rather than Rust enums (Q60). None of the
//! description's thirty-three enumerations is a closed set; each is a list of
//! what exists today, so the values live in a `&[&str]` constant beside the
//! field — which is what a tool's parameter description quotes — and the
//! drift test asserts the constant still says what the document says.
//!
//! [`shapes`] is what makes that test possible: the `model!` macro writes the
//! struct and the list of JSON names from one source, so a field cannot be
//! deleted from a model and left in the table (Q61).

use std::collections::BTreeMap;

use serde_json::Value;

/// Whatever the description did not mention.
///
/// A `BTreeMap` rather than a `HashMap` so that a model serialised back out
/// puts its unknown fields in a stable order, which keeps a tool's answer the
/// same from one call to the next.
pub type Unknown = BTreeMap<String, Value>;

/// One model, as the drift test sees it.
///
/// `schema` is the path the description reaches this object by: a name from
/// `components/schemas` for the thirty-four that have one, a dotted path like
/// `Device.clientConnectivity` for the eleven inline objects that do not, and
/// a route like `POST /tailnet/{tailnet}/keys body` for one the description
/// spells out where it is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelShape {
    pub schema: &'static str,
    /// The JSON names, in declaration order. Not the Rust names.
    pub fields: &'static [&'static str],
}

/// Declare the model modules, and the two tables the drift test reads.
///
/// The list appears once. Written out three times — once to declare the
/// modules, once for [`shapes`], once for [`known_values`] — a module added to
/// one and forgotten in the others would be a model the drift test never
/// checks, which is a green test over an unchecked shape.
macro_rules! modules {
    ($($module:ident),* $(,)?) => {
        $(pub mod $module;)*

        /// Every model in this crate, for the drift test to check the
        /// description against.
        pub fn shapes() -> impl Iterator<Item = &'static ModelShape> {
            [$($module::SHAPES),*].into_iter().flatten()
        }

        /// Every documented string in this crate. The drift test requires this
        /// to cover the description's enumerations exactly — no path missing,
        /// no path left over, and every list equal.
        pub fn known_values() -> impl Iterator<Item = &'static KnownValues> {
            [$($module::KNOWN_VALUES),*].into_iter().flatten()
        }
    };
}

modules!(
    device, dns, key, logging, policy, service, tailnet, user, webhook
);

/// A documented string's known values, keyed by where the description puts it.
///
/// The path is where the description puts the enumeration: `Key.keyType` for a
/// property of a named schema, `Webhook.subscriptions[]` for an array's items,
/// `LogType` for an enumeration that is a schema in its own right, `?fields`
/// for a parameter the whole document shares, and `POST
/// /tailnet/{tailnet}/keys body.keyType` for one belonging to a single call.
pub type KnownValues = (&'static str, &'static [&'static str]);

/// Declare models.
///
/// One block per module. Each entry names the Rust type, the path the
/// description reaches it by when that differs from the type's name, and its
/// fields as `rust_name: "jsonName" => Type`. The JSON name is written once
/// and becomes both the `serde` rename and the row in [`SHAPES`], which is
/// what stops the two drifting apart (Q61).
///
/// ```ignore
/// model! {
///     /// A machine in the tailnet.
///     Device {
///         id: "id" => String,
///         node_id: "nodeId" => String,
///     }
///
///     /// How this device is reaching the network.
///     ClientConnectivity as "Device.clientConnectivity" {
///         endpoints: "endpoints" => Vec<String>,
///     }
///
///     /// The same six fields, so the same struct.
///     VipServiceInfoPut as "VIPServiceInfoPut" is VipServiceInfo;
/// }
/// ```
///
/// Every field is optional and every type is the one inside the `Option`: the
/// control plane omits what does not apply, and a model that demanded a field
/// would fail to parse a body a caller could have used.
///
/// The last form declares a schema that some other model already has the shape
/// of. The description does that where one body is another under a second
/// name, and a second struct there would be six duplicated fields and a
/// conversion between them that says nothing.
///
/// [`SHAPES`]: crate::models::ModelShape
#[macro_export]
macro_rules! model {
    ($($declarations:tt)*) => {
        $crate::model_entries! { @shapes[] $($declarations)* }
    };
}

/// [`model!`]'s worker, which reads one declaration at a time so that a
/// struct and a bare shape can sit in the same block.
#[doc(hidden)]
#[macro_export]
macro_rules! model_entries {
    // A struct, and the shape that describes it.
    (
        @shapes[$($shape:expr,)*]
        $(#[doc = $doc:literal])*
        $name:ident $(as $schema:literal)? {
            $(
                $(#[doc = $field_doc:literal])*
                $field:ident : $json:literal => $ty:ty
            ),* $(,)?
        }
        $($rest:tt)*
    ) => {
        $(#[doc = $doc])*
        #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
        pub struct $name {
            $(
                $(#[doc = $field_doc])*
                #[serde(rename = $json, default, skip_serializing_if = "Option::is_none")]
                pub $field: ::std::option::Option<$ty>,
            )*
            /// Everything the description did not mention, kept so that a
            /// control plane ahead of this build still answers usefully.
            #[serde(flatten)]
            pub unknown: $crate::models::Unknown,
        }

        impl $name {
            /// The path the vendored description reaches this by.
            pub const SCHEMA: &'static str = {
                #[allow(unused_mut, unused_assignments)]
                let mut schema = ::std::stringify!($name);
                $( schema = $schema; )?
                schema
            };

            /// The JSON names of the fields above, in declaration order.
            /// Not the Rust names.
            pub const FIELDS: &'static [&'static str] = &[$($json),*];
        }

        $crate::model_entries! {
            @shapes[
                $($shape,)*
                $crate::models::ModelShape {
                    schema: <$name>::SCHEMA,
                    fields: <$name>::FIELDS,
                },
            ]
            $($rest)*
        }
    };

    // A schema another model already has the shape of.
    (
        @shapes[$($shape:expr,)*]
        $(#[doc = $doc:literal])*
        $alias:ident as $schema:literal is $name:ident;
        $($rest:tt)*
    ) => {
        $(#[doc = $doc])*
        pub type $alias = $name;

        $crate::model_entries! {
            @shapes[
                $($shape,)*
                $crate::models::ModelShape { schema: $schema, fields: <$name>::FIELDS },
            ]
            $($rest)*
        }
    };

    // Nothing left to read.
    (@shapes[$($shape:expr,)*]) => {
        /// Every model declared in this module, in declaration order.
        pub const SHAPES: &[$crate::models::ModelShape] = &[$($shape,)*];
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_the_description_never_mentioned_survives_a_round_trip() {
        // The criterion in its smallest form: a body from a control plane
        // ahead of this build parses, the new field is readable, and it is
        // still there when the model is handed on to a caller.
        let body = serde_json::json!({
            "id": "example-1",
            "hostname": "laptop",
            "quantumEntangled": true,
        });
        let device: device::Device = serde_json::from_value(body.clone()).expect("it parses");
        assert_eq!(device.id.as_deref(), Some("example-1"));
        assert_eq!(
            device.unknown.get("quantumEntangled"),
            Some(&Value::Bool(true)),
            "the field is retrievable, not merely tolerated"
        );
        assert_eq!(
            serde_json::to_value(&device).expect("it serialises"),
            body,
            "and it comes back out as it went in"
        );
    }

    #[test]
    fn a_field_that_is_absent_is_absent_rather_than_null() {
        // `skip_serializing_if` is what makes a model usable as a request
        // body: a `PATCH` that spelled every unset field as `null` would be
        // asking the control plane to clear them.
        let empty = device::Device {
            id: Some("example-1".to_owned()),
            ..serde_json::from_value(serde_json::json!({})).expect("an empty body is a device")
        };
        assert_eq!(
            serde_json::to_value(&empty).expect("it serialises"),
            serde_json::json!({"id": "example-1"})
        );
    }

    #[test]
    fn every_model_names_a_distinct_schema() {
        // Two models claiming one path would have the drift test check one of
        // them twice and the other never.
        let mut seen = std::collections::BTreeSet::new();
        for shape in shapes() {
            assert!(
                seen.insert(shape.schema),
                "{} is declared twice",
                shape.schema
            );
        }
        let mut paths = std::collections::BTreeSet::new();
        for (path, _) in known_values() {
            assert!(paths.insert(*path), "{path} is declared twice");
        }
    }

    #[test]
    fn a_secret_field_is_not_printed_by_a_model() {
        // The reason `Secret` is in a model at all (Q62): these structs derive
        // `Debug`, and a minted key reaching a `tracing` field is how it ends
        // up in a log.
        let minted: key::Key = serde_json::from_value(serde_json::json!({
            "id": "kExAmPlE",
            "key": "tskey-auth-example1CNTRL-secretpart",
        }))
        .expect("it parses");
        assert!(!format!("{minted:?}").contains("secretpart"), "{minted:?}");
        assert_eq!(
            minted.key.as_ref().map(crate::Secret::expose),
            Some("tskey-auth-example1CNTRL-secretpart"),
            "and is still readable by whoever asked for it"
        );
    }
}
