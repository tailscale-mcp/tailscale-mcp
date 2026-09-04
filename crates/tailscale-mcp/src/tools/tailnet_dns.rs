//! The tailnet's DNS: nameservers, search paths, split DNS and MagicDNS.
//!
//! Eleven endpoints describing one thing two ways. The older six take the
//! pieces separately — nameservers here, search paths there, MagicDNS in a
//! third place — while `dns/configuration` is all of it in one document, and
//! spells split DNS as objects where the older endpoint spells it as bare
//! addresses. Both are still served, so both are here, unrenamed (ADR-0004).
//!
//! **Replace means replace.** Six of these eleven overwrite a whole list or a
//! whole document; only `tailnet_dns_split_update` merges. A tool named `_set`
//! that silently discarded the nameservers a caller did not mention would be
//! the most expensive kind of surprise on this surface, so the name says which
//! it is: `_replace` overwrites, `_update` merges, and `_set` is reserved for
//! the one endpoint that carries a single value (Q72).
//!
//! **MagicDNS follows the nameservers.** Removing the last global nameserver
//! turns MagicDNS off, and turning MagicDNS on without one is refused by the
//! control plane. Neither is enforced here — the control plane owns that rule
//! and states it better than a guess would — but both are in the descriptions,
//! because an agent that reads them will not have to learn it from a failure.

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context::ToolContext;
use crate::error::{ToolError, ToolResult};

crate::tools! {
    /// Read the tailnet's global DNS nameservers. Answers `{"dns": [...]}`.
    tailnet_dns_nameservers_get => NoParams, nameservers_get,
        toolset: TailnetDns, tier: Read, idempotent: true;

    /// Replace the tailnet's global nameservers with exactly this list.
    ///
    /// A full replace: anything not in `dns` is removed. An empty list removes
    /// every global nameserver, which also turns MagicDNS off — the answer
    /// says which state MagicDNS was left in.
    tailnet_dns_nameservers_replace => NameserversParams, nameservers_replace,
        toolset: TailnetDns, tier: Write, idempotent: true;

    /// Read whether MagicDNS is on for the tailnet.
    tailnet_dns_preferences_get => NoParams, preferences_get,
        toolset: TailnetDns, tier: Read, idempotent: true;

    /// Turn MagicDNS on or off for the tailnet.
    ///
    /// Turning it on needs at least one global nameserver; without one the
    /// control plane refuses the call.
    tailnet_dns_preferences_set => PreferencesParams, preferences_set,
        toolset: TailnetDns, tier: Write, idempotent: true;

    /// Read the search domains appended to a bare hostname.
    tailnet_dns_search_paths_get => NoParams, search_paths_get,
        toolset: TailnetDns, tier: Read, idempotent: true;

    /// Replace the search domains with exactly this list.
    ///
    /// A full replace: a domain not in `search_paths` is removed, and an empty
    /// list removes all of them.
    tailnet_dns_search_paths_replace => SearchPathsParams, search_paths_replace,
        toolset: TailnetDns, tier: Write, idempotent: true;

    /// Read the split-DNS map: which nameservers answer for which domain.
    tailnet_dns_split_get => NoParams, split_get,
        toolset: TailnetDns, tier: Read, idempotent: true;

    /// Change the split-DNS entries named here and leave the rest alone.
    ///
    /// A merge, and the only one of these that is: a domain the map does not
    /// mention keeps its nameservers. A domain mapped to `null` has its
    /// nameservers cleared.
    tailnet_dns_split_update => SplitParams, split_update,
        toolset: TailnetDns, tier: Write, idempotent: true;

    /// Replace the whole split-DNS map with exactly this one.
    ///
    /// A full replace: a domain not named here loses its nameservers, and an
    /// empty map clears every domain.
    tailnet_dns_split_replace => SplitParams, split_replace,
        toolset: TailnetDns, tier: Write, idempotent: true;

    /// Read the whole DNS configuration in one document: nameservers, split
    /// DNS, search paths and preferences.
    ///
    /// The newer shape, and the one to read before a `_configuration_replace`.
    /// Its split DNS is a map of domain to resolver objects, where
    /// `tailnet_dns_split_get` gives bare addresses for the same thing.
    tailnet_dns_configuration_get => NoParams, configuration_get,
        toolset: TailnetDns, tier: Read, idempotent: true;

    /// Replace the whole DNS configuration.
    ///
    /// A full replace of everything the document holds — nameservers, split
    /// DNS, search paths and preferences together. Read
    /// `tailnet_dns_configuration_get` first and send that back with the one
    /// change made, or anything omitted is cleared.
    tailnet_dns_configuration_replace => ConfigurationParams, configuration_replace,
        toolset: TailnetDns, tier: Write, idempotent: true;
}

/// `/api/v2/tailnet/<tailnet>/dns/<rest>`, which every one of these is under.
fn dns_path(client: &tailscale_rest::Client, rest: &str) -> String {
    client.tailnet_path(None, &format!("/dns{rest}"))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoParams {}

// ---------------------------------------------------------------------------
// Nameservers
// ---------------------------------------------------------------------------

async fn nameservers_get(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(dns_path(client, "/nameservers"))
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NameserversParams {
    /// The complete list of global nameservers, as addresses rather than
    /// URLs. An empty list removes them all and turns MagicDNS off.
    pub dns: Vec<String>,
}

/// The body the nameserver endpoint takes, which is its answer's shape too.
#[derive(Debug, Serialize)]
struct Nameservers {
    dns: Vec<String>,
}

async fn nameservers_replace(ctx: &ToolContext, params: NameserversParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let body = Nameservers {
        dns: addresses(params.dns)?,
    };
    Ok(client
        .post(dns_path(client, "/nameservers"))
        .json(&body)
        .send_as::<Value>()
        .await?)
}

/// Nameserver addresses, trimmed, with nothing empty among them.
///
/// An empty string in the list is not the same as an empty list: the empty
/// list is how a caller removes every nameserver, which is documented and
/// deliberate, while a blank entry is a mistake the control plane would answer
/// with a 400 that does not say which entry.
fn addresses(given: Vec<String>) -> ToolResult<Vec<String>> {
    given
        .into_iter()
        .map(|address| {
            let trimmed = address.trim();
            if trimmed.is_empty() {
                return Err(ToolError::invalid_args(
                    "`dns` has an empty entry; send `[]` to remove every nameserver",
                ));
            }
            Ok(trimmed.to_owned())
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Preferences
// ---------------------------------------------------------------------------

async fn preferences_get(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(dns_path(client, "/preferences"))
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PreferencesParams {
    /// Whether MagicDNS is on. Turning it on needs at least one global
    /// nameserver to already be set.
    pub magic_dns: bool,
}

async fn preferences_set(ctx: &ToolContext, params: PreferencesParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let body = tailscale_rest::models::dns::DnsPreferences {
        magic_dns: Some(params.magic_dns),
        unknown: Default::default(),
    };
    Ok(client
        .post(dns_path(client, "/preferences"))
        .json(&body)
        .send_as::<Value>()
        .await?)
}

// ---------------------------------------------------------------------------
// Search paths
// ---------------------------------------------------------------------------

async fn search_paths_get(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(dns_path(client, "/searchpaths"))
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchPathsParams {
    /// The complete list of search domains. An empty list removes them all.
    pub search_paths: Vec<String>,
}

async fn search_paths_replace(ctx: &ToolContext, params: SearchPathsParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    let body = tailscale_rest::models::dns::DnsSearchPaths {
        search_paths: Some(addresses(params.search_paths)?),
        unknown: Default::default(),
    };
    Ok(client
        .post(dns_path(client, "/searchpaths"))
        .json(&body)
        .send_as::<Value>()
        .await?)
}

// ---------------------------------------------------------------------------
// Split DNS
// ---------------------------------------------------------------------------

async fn split_get(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(dns_path(client, "/split-dns"))
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SplitParams {
    /// Domain suffix to the nameservers that answer for it, as
    /// `{"example.com": ["10.0.0.1"]}`. A domain mapped to `null` has its
    /// nameservers cleared.
    pub domains: tailscale_rest::models::dns::SplitDns,
}

async fn split_update(ctx: &ToolContext, params: SplitParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .patch(dns_path(client, "/split-dns"))
        .json(&params.domains)
        .send_as::<Value>()
        .await?)
}

async fn split_replace(ctx: &ToolContext, params: SplitParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .put(dns_path(client, "/split-dns"))
        .json(&params.domains)
        .send_as::<Value>()
        .await?)
}

// ---------------------------------------------------------------------------
// The whole configuration
// ---------------------------------------------------------------------------

async fn configuration_get(ctx: &ToolContext, _params: NoParams) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    Ok(client
        .get(dns_path(client, "/configuration"))
        .send_as::<Value>()
        .await?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfigurationParams {
    /// The whole configuration, in the shape `tailnet_dns_configuration_get`
    /// answers with: `nameservers` as `{"address": ..., "useWithExitNode":
    /// ...}` objects, `splitDNS` as a map of domain to those same objects,
    /// `searchPaths` as a list, and `preferences` as `{"overrideLocalDNS":
    /// ..., "magicDNS": ...}`. Sent to the control plane as written, so the
    /// field names here are Tailscale's own rather than this server's
    /// (ADR-0004).
    pub configuration: Value,
}

async fn configuration_replace(
    ctx: &ToolContext,
    params: ConfigurationParams,
) -> ToolResult<Value> {
    let client = ctx.tailnet()?;
    // The document goes through unrenamed, so the one thing checked is that it
    // is a document at all: a list or a string here would be a caller that
    // passed the wrong argument, and a 400 quoting no field is a worse way to
    // find that out.
    if !params.configuration.is_object() {
        return Err(ToolError::invalid_args(
            "`configuration` is the DNS configuration document, an object with `nameservers`,              `splitDNS`, `searchPaths` and `preferences`",
        )
        .with_hint("Call `tailnet_dns_configuration_get` and send back what it answered."));
    }
    Ok(client
        .post(dns_path(client, "/configuration"))
        .json(&params.configuration)
        .send_as::<Value>()
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_nameserver_is_refused_but_an_empty_list_is_not() {
        // `[]` is documented: it is how every nameserver is removed. `[""]`
        // is a caller that meant `[]` and got it wrong, and the control plane
        // would answer that with a 400 naming nothing.
        assert_eq!(
            addresses(Vec::new()).expect("an empty list"),
            Vec::<String>::new()
        );
        assert_eq!(
            addresses(vec![" 8.8.8.8 ".to_owned()]).expect("one address"),
            ["8.8.8.8"]
        );
        assert!(addresses(vec!["8.8.8.8".to_owned(), "  ".to_owned()]).is_err());
    }
}
