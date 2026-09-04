//! The tool table, and the macro that fills it.
//!
//! Every tool is declared once. That declaration produces its parameter type's
//! schema, the shim that deserialises into it, and the metadata row — so there
//! is no way to add a tool and forget to classify it, and no second list to
//! fall out of step with the first.
//!
//! The router is this table. `list_tools` filters it through the
//! [`Gate`](crate::gating::Gate); `call_tool` looks a name up in it. There is
//! no parallel registration anywhere.

use std::collections::BTreeMap;
use std::sync::Arc;

use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::Value;
use tailscale_cli::BoxFuture;

use crate::context::ToolContext;
use crate::error::{ToolError, ToolResult};
use crate::gating::Gate;
use crate::meta::ToolMeta;

/// The name of the parameter that carries the caller's intent.
///
/// Injected into the schema of every tool that requires it, rather than being
/// repeated in twenty parameter structs, so that the rule and its enforcement
/// are the same line of code.
pub const CONFIRM_PARAM: &str = "confirm";

/// The longest name the protocol permits.
pub const MAX_NAME_LEN: usize = 128;

/// How a tool is actually run.
pub type InvokeFn = fn(Arc<ToolContext>, JsonObject) -> BoxFuture<'static, ToolResult<Value>>;

/// One tool: its row in the table, its schema, and its handler.
#[derive(Clone)]
pub struct ToolEntry {
    pub meta: ToolMeta,
    /// Built on demand and cached by the SDK, keyed on the parameter type.
    pub schema: fn() -> Result<Arc<JsonObject>, String>,
    pub invoke: InvokeFn,
}

impl std::fmt::Debug for ToolEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolEntry")
            .field("meta", &self.meta)
            .finish_non_exhaustive()
    }
}

impl ToolEntry {
    /// The tool as a client sees it.
    ///
    /// Two things are added here rather than at the declaration: the
    /// annotations, which are derived from the tier so they cannot contradict
    /// it, and the confirmation parameter, which is added to exactly the tools
    /// whose row asks for it.
    pub fn describe(&self) -> Result<Tool, String> {
        let mut schema = (self.schema)()?.as_ref().clone();
        if self.meta.requires_confirmation {
            add_confirm_property(&mut schema);
        }

        let a = self.meta.annotations();
        Ok(
            Tool::new(self.meta.name, self.meta.summary, Arc::new(schema)).with_annotations(
                ToolAnnotations::new()
                    .read_only(a.read_only)
                    .destructive(a.destructive)
                    .idempotent(a.idempotent)
                    .open_world(a.open_world),
            ),
        )
    }
}

/// Add the confirmation flag to a generated schema.
fn add_confirm_property(schema: &mut JsonObject) {
    let properties = schema
        .entry("properties")
        .or_insert_with(|| Value::Object(JsonObject::new()));
    if let Some(properties) = properties.as_object_mut() {
        properties.insert(
            CONFIRM_PARAM.to_owned(),
            serde_json::json!({
                "type": "boolean",
                "default": false,
                "description":
                    "Set to true to confirm this operation. It is irreversible, \
                     or it can disconnect the node this server runs on, so it \
                     will not run without an explicit intent.",
            }),
        );
    }
}

/// The tool table.
///
/// Built once from the declarations, then shared. Lookup is by name because
/// that is what a call carries; listing walks the ordered vector so that the
/// output is stable between runs.
#[derive(Debug, Clone)]
pub struct Registry {
    entries: Vec<ToolEntry>,
    by_name: BTreeMap<&'static str, usize>,
}

impl Registry {
    /// Build a registry, rejecting a table that could not be served correctly.
    ///
    /// The checks are cheap and run at startup rather than in a test alone,
    /// because a duplicate name silently shadowing a tool is the kind of bug
    /// that survives a release.
    pub fn new(entries: Vec<ToolEntry>) -> Result<Self, RegistryError> {
        let mut by_name = BTreeMap::new();
        for (index, entry) in entries.iter().enumerate() {
            let name = entry.meta.name;
            validate_name(name)?;
            if by_name.insert(name, index).is_some() {
                return Err(RegistryError::DuplicateName(name));
            }
            if entry.meta.self_severing && !entry.meta.requires_confirmation {
                return Err(RegistryError::SelfSeveringWithoutConfirmation(name));
            }
            if let Err(reason) = (entry.schema)() {
                return Err(RegistryError::BadSchema { name, reason });
            }
        }
        Ok(Self { entries, by_name })
    }

    /// The metadata rows, for anything that needs the table without the
    /// handlers: the gate's emptiness check, the `tools` subcommand, the
    /// generated documentation.
    pub fn metas(&self) -> Vec<ToolMeta> {
        self.entries.iter().map(|e| e.meta).collect()
    }

    pub fn entries(&self) -> &[ToolEntry] {
        &self.entries
    }

    pub fn get(&self, name: &str) -> Option<&ToolEntry> {
        self.by_name.get(name).map(|index| &self.entries[*index])
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The tools this server offers, in name order.
    ///
    /// A tool the gate does not permit is absent, not present-and-failing: a
    /// model that can see a tool will reach for it.
    pub fn visible(&self, gate: &Gate) -> Vec<&ToolEntry> {
        let mut visible: Vec<&ToolEntry> = self
            .entries
            .iter()
            .filter(|entry| gate.permits(&entry.meta))
            .collect();
        visible.sort_by_key(|entry| entry.meta.name);
        visible
    }

    /// Look a call up, applying the gate and the confirmation rule.
    ///
    /// Returns the entry and the arguments with the confirmation flag removed,
    /// so a handler never has to know the flag exists.
    pub fn resolve(
        &self,
        name: &str,
        mut args: JsonObject,
        gate: &Gate,
    ) -> ToolResult<(&ToolEntry, JsonObject)> {
        let entry = self
            .get(name)
            .ok_or_else(|| ToolError::not_found(&format!("the tool `{name}`")))?;

        if !gate.permits(&entry.meta) {
            return Err(ToolError::not_permitted(name, &gate.needs(&entry.meta)));
        }

        if entry.meta.requires_confirmation {
            let confirmed = match args.remove(CONFIRM_PARAM) {
                Some(Value::Bool(confirmed)) => confirmed,
                None | Some(Value::Null) => false,
                Some(other) => {
                    return Err(ToolError::invalid_args(format!(
                        "`{CONFIRM_PARAM}` must be true or false, not {other}"
                    )));
                }
            };
            if !confirmed {
                return Err(ToolError::confirmation_required(
                    name,
                    confirmation_consequence(&entry.meta),
                ));
            }
        }

        Ok((entry, args))
    }
}

/// The clause that follows the tool name in a confirmation message.
fn confirmation_consequence(meta: &crate::meta::ToolMeta) -> &'static str {
    if meta.self_severing {
        "can disconnect the node this server runs on, so it needs `confirm: true`"
    } else {
        "cannot be undone, so it needs `confirm: true`"
    }
}

/// A table that could not be served.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    #[error("two tools are both named `{0}`")]
    DuplicateName(&'static str),

    #[error("the tool name `{name}` {reason}")]
    BadName {
        name: &'static str,
        reason: &'static str,
    },

    #[error("`{0}` is self-severing but does not require confirmation")]
    SelfSeveringWithoutConfirmation(&'static str),

    #[error("the parameters of `{name}` do not make a valid input schema: {reason}")]
    BadSchema { name: &'static str, reason: String },
}

/// The protocol's rules for a tool name, plus ours.
fn validate_name(name: &'static str) -> Result<(), RegistryError> {
    let bad = |reason| Err(RegistryError::BadName { name, reason });
    if name.is_empty() {
        return bad("is empty");
    }
    if name.len() > MAX_NAME_LEN {
        return bad("is longer than the 128 characters the protocol allows");
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
    {
        return bad("contains a character outside [A-Za-z0-9_.-]");
    }
    if !name.starts_with("tailscale_") && !name.starts_with("tailnet_") {
        return bad("does not begin with `tailscale_` or `tailnet_`");
    }
    Ok(())
}

/// Declare tools.
///
/// One block per toolset module; one entry per tool. The entry names its
/// parameter type and its handler, and states its toolset and tier — omit
/// either and this does not compile, which is the point.
///
/// ```ignore
/// tools! {
///     /// Report the state of the local node and the peers it knows about.
///     tailscale_status => StatusParams, handlers::status,
///         toolset: LocalStatus, tier: Read, idempotent: true;
/// }
/// ```
///
/// Optional trailing settings, each defaulting to the safe answer:
/// `idempotent` (false), `confirm` (false), `severing` (false, and implies
/// `confirm`), `since` (no minimum version).
#[macro_export]
macro_rules! tools {
    (
        $(
            $(#[doc = $summary:literal])+
            $name:ident => $params:ty, $handler:path,
                toolset: $toolset:ident,
                tier: $tier:ident
                $(, idempotent: $idempotent:literal)?
                $(, confirm: $confirm:literal)?
                $(, severing: $severing:literal)?
                $(, since: $since:literal)?
                $(,)?
            ;
        )*
    ) => {
        /// The metadata of each tool declared in this module, one constant per
        /// tool, so that a handler needing its own row — for the version it
        /// requires, or for its name in an error — names it rather than
        /// searching for it.
        pub mod metas {
            #![allow(non_upper_case_globals)]

            $(
                #[doc = ::std::concat!($($summary),*)]
                pub const $name: $crate::meta::ToolMeta = {
                    #[allow(unused_mut, unused_assignments)]
                    let mut severing = false;
                    $( severing = $severing; )?
                    #[allow(unused_mut, unused_assignments)]
                    let mut confirm = severing;
                    $( confirm = $confirm || severing; )?
                    #[allow(unused_mut, unused_assignments)]
                    let mut idempotent = false;
                    $( idempotent = $idempotent; )?
                    #[allow(unused_mut, unused_assignments)]
                    let mut since: ::std::option::Option<&'static str> =
                        ::std::option::Option::None;
                    $( since = ::std::option::Option::Some($since); )?

                    $crate::meta::ToolMeta {
                        name: ::std::stringify!($name),
                        toolset: $crate::meta::Toolset::$toolset,
                        tier: $crate::meta::Tier::$tier,
                        summary: ::std::concat!($($summary),*).trim_ascii(),
                        self_severing: severing,
                        requires_confirmation: confirm,
                        idempotent,
                        min_version: since,
                    }
                };
            )*
        }

        /// Every tool declared in this module, in declaration order.
        pub fn entries() -> ::std::vec::Vec<$crate::registry::ToolEntry> {
            ::std::vec![
                $(
                    {
                        // Each tool gets its own shim so that the table can
                        // hold plain function pointers rather than boxed
                        // closures over twenty different parameter types.
                        fn invoke(
                            ctx: ::std::sync::Arc<$crate::context::ToolContext>,
                            args: ::rmcp::model::JsonObject,
                        ) -> ::tailscale_cli::BoxFuture<
                            'static,
                            $crate::error::ToolResult<::serde_json::Value>,
                        > {
                            ::std::boxed::Box::pin(async move {
                                let params: $params = $crate::registry::parse_params(
                                    ::std::stringify!($name),
                                    args,
                                )?;
                                $handler(&ctx, params).await
                            })
                        }

                        $crate::registry::ToolEntry {
                            meta: self::metas::$name,
                            schema: || {
                                ::rmcp::handler::server::tool::schema_for_input::<$params>()
                            },
                            invoke,
                        }
                    },
                )*
            ]
        }
    };
}

/// Turn a call's arguments into a handler's parameter type.
///
/// A shape mismatch is `invalid_args` rather than a protocol error: the caller
/// is a model, and a structured answer naming the problem is something it can
/// act on, whereas a JSON-RPC error is opaque to it.
pub fn parse_params<T: serde::de::DeserializeOwned>(tool: &str, args: JsonObject) -> ToolResult<T> {
    serde_json::from_value(Value::Object(args)).map_err(|e| {
        ToolError::invalid_args(format!("`{tool}` was called with unusable arguments: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::{Tier, Toolset};
    use schemars::JsonSchema;
    use serde::Deserialize;
    use std::collections::BTreeSet;

    #[derive(Debug, Deserialize, JsonSchema)]
    struct Empty {}

    /// A parameter struct exercising both naming conventions in one tool, as
    /// ADR-0004 requires: what we own is snake_case, what Tailscale owns keeps
    /// the shape Tailscale gave it.
    // The fields exist to be reflected into a schema, not to be read.
    #[allow(dead_code)]
    #[derive(Debug, Deserialize, JsonSchema)]
    struct MixedParams {
        /// Ours: how the result should be shaped.
        include_peers: bool,
        /// Theirs: forwarded to the control plane unchanged.
        #[serde(rename = "keyExpiryDisabled")]
        key_expiry_disabled: bool,
    }

    async fn ok_handler<P>(_ctx: &ToolContext, _params: P) -> ToolResult<Value> {
        Ok(Value::Null)
    }

    mod declared {
        use super::*;

        crate::tools! {
            /// Report the state of the local node.
            tailscale_status => Empty, super::ok_handler,
                toolset: LocalStatus, tier: Read, idempotent: true;

            /// Disconnect this node from the tailnet.
            tailscale_down => Empty, super::ok_handler,
                toolset: LocalPrefs, tier: Destructive, severing: true;

            /// Delete a device from the tailnet.
            tailnet_device_delete => MixedParams, super::ok_handler,
                toolset: TailnetDevices, tier: Destructive, confirm: true, since: "1.60";
        }
    }

    fn registry() -> Registry {
        Registry::new(declared::entries()).expect("a well-formed table")
    }

    fn open_gate() -> Gate {
        Gate::unchecked(
            Toolset::ALL.iter().copied().collect(),
            Tier::Destructive,
            BTreeSet::new(),
        )
    }

    #[test]
    fn a_declaration_produces_its_metadata_row() {
        let registry = registry();
        assert_eq!(registry.len(), 3);

        let status = registry.get("tailscale_status").expect("declared");
        assert_eq!(status.meta.toolset, Toolset::LocalStatus);
        assert_eq!(status.meta.tier, Tier::Read);
        assert_eq!(status.meta.summary, "Report the state of the local node.");
        assert!(status.meta.idempotent);
        assert!(!status.meta.requires_confirmation);
        assert_eq!(status.meta.min_version, None);
    }

    #[test]
    fn a_self_severing_tool_requires_confirmation_without_being_told_twice() {
        let down = registry().get("tailscale_down").expect("declared").meta;
        assert!(down.self_severing);
        assert!(
            down.requires_confirmation,
            "severing must imply confirmation"
        );
    }

    #[test]
    fn optional_settings_land_where_they_are_given() {
        let delete = registry()
            .get("tailnet_device_delete")
            .expect("declared")
            .meta;
        assert!(delete.requires_confirmation);
        assert!(!delete.self_severing);
        assert_eq!(delete.min_version, Some("1.60"));
    }

    #[test]
    fn every_tool_has_exactly_one_row_and_a_usable_name() {
        let registry = registry();
        let mut names: Vec<&str> = registry.entries().iter().map(|e| e.meta.name).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(before, names.len(), "a tool appears twice");

        for name in names {
            assert!(!name.is_empty());
            assert!(name.len() <= MAX_NAME_LEN, "{name} is too long");
            assert!(
                name.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.'),
                "{name} uses a character the protocol does not allow"
            );
            assert!(
                name.starts_with("tailscale_") || name.starts_with("tailnet_"),
                "{name} does not say which surface it acts on"
            );
        }
    }

    #[test]
    fn a_duplicate_name_is_refused_at_construction() {
        let mut entries = declared::entries();
        entries.push(entries[0].clone());
        assert_eq!(
            Registry::new(entries).err(),
            Some(RegistryError::DuplicateName("tailscale_status"))
        );
    }

    #[test]
    fn annotations_are_derived_from_the_tier_not_declared_beside_it() {
        let registry = registry();
        let status = registry
            .get("tailscale_status")
            .expect("declared")
            .describe()
            .expect("a valid schema");
        let a = status.annotations.expect("annotations are set");
        assert_eq!(a.read_only_hint, Some(true));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(true));

        let down = registry
            .get("tailscale_down")
            .expect("declared")
            .describe()
            .expect("a valid schema");
        let a = down.annotations.expect("annotations are set");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.destructive_hint, Some(true));
    }

    #[test]
    fn both_naming_conventions_survive_schema_generation() {
        let tool = registry()
            .get("tailnet_device_delete")
            .expect("declared")
            .describe()
            .expect("a valid schema");
        let properties = tool
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
            .expect("an object schema with properties");

        // Ours, in the casing we chose.
        assert!(
            properties.contains_key("include_peers"),
            "server-owned parameters stay snake_case: {properties:?}"
        );
        // Theirs, in the casing they chose.
        assert!(
            properties.contains_key("keyExpiryDisabled"),
            "control-plane fields keep their own shape: {properties:?}"
        );
    }

    #[test]
    fn the_confirmation_flag_appears_only_where_it_is_required() {
        let registry = registry();
        let confirming = registry
            .get("tailscale_down")
            .expect("declared")
            .describe()
            .expect("a valid schema");
        let properties = confirming
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
            .expect("properties");
        assert!(properties.contains_key(CONFIRM_PARAM));

        let plain = registry
            .get("tailscale_status")
            .expect("declared")
            .describe()
            .expect("a valid schema");
        // A parameterless tool need not have a `properties` key at all; what
        // matters is that nothing added one behind our back.
        let properties = plain
            .input_schema
            .get("properties")
            .and_then(Value::as_object);
        assert!(properties.is_none_or(|p| !p.contains_key(CONFIRM_PARAM)));
    }

    #[test]
    fn a_confirming_tool_refuses_until_the_caller_says_so() {
        let registry = registry();
        let gate = open_gate();

        let err = registry
            .resolve("tailscale_down", JsonObject::new(), &gate)
            .expect_err("must not run unconfirmed");
        assert_eq!(err.code, crate::error::ErrorCode::ConfirmationRequired);
        assert!(err.message.contains("disconnect"), "{}", err.message);

        let mut args = JsonObject::new();
        args.insert(CONFIRM_PARAM.to_owned(), Value::Bool(true));
        let (entry, rest) = registry
            .resolve("tailscale_down", args, &gate)
            .expect("a confirmed call runs");
        assert_eq!(entry.meta.name, "tailscale_down");
        assert!(
            !rest.contains_key(CONFIRM_PARAM),
            "the handler should not see the flag"
        );
    }

    #[test]
    fn a_confirmation_flag_of_the_wrong_shape_is_a_bad_argument() {
        let mut args = JsonObject::new();
        args.insert(CONFIRM_PARAM.to_owned(), Value::String("yes".to_owned()));
        let err = registry()
            .resolve("tailscale_down", args, &open_gate())
            .expect_err("`yes` is not a boolean");
        assert_eq!(err.code, crate::error::ErrorCode::InvalidArgs);
    }

    #[test]
    fn the_router_is_the_table_filtered_by_the_gate() {
        let registry = registry();
        let gate = Gate::unchecked(
            BTreeSet::from([Toolset::LocalStatus, Toolset::LocalPrefs]),
            Tier::Read,
            BTreeSet::new(),
        );
        let visible: Vec<&str> = registry
            .visible(&gate)
            .iter()
            .map(|e| e.meta.name)
            .collect();
        assert_eq!(visible, ["tailscale_status"]);

        // Hidden, but a call still gets a reasoned refusal rather than a
        // confusing "no such tool".
        let err = registry
            .resolve("tailscale_down", JsonObject::new(), &gate)
            .expect_err("hidden tools do not run");
        assert_eq!(err.code, crate::error::ErrorCode::NotPermitted);
        assert!(
            err.hint
                .as_deref()
                .is_some_and(|h| h.contains("--allow-destructive")),
            "{err:?}"
        );
    }

    #[test]
    fn an_unknown_name_is_not_found() {
        let err = registry()
            .resolve("tailscale_nonesuch", JsonObject::new(), &open_gate())
            .expect_err("no such tool");
        assert_eq!(err.code, crate::error::ErrorCode::NotFound);
    }

    #[test]
    fn visible_tools_are_listed_in_a_stable_order() {
        let registry = registry();
        let names: Vec<&str> = registry
            .visible(&open_gate())
            .iter()
            .map(|e| e.meta.name)
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }
}
