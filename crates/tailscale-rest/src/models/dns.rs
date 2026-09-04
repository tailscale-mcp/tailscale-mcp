//! The tailnet's DNS: nameservers, search paths, split DNS and MagicDNS.
//!
//! The description carries two generations of this. The older endpoints take
//! and return the pieces separately — [`DnsPreferences`], [`DnsSearchPaths`]
//! and a `SplitDns` map of domain to plain address strings — while
//! [`DnsConfiguration`] is the whole thing at once, and spells its split DNS
//! as a map of domain to [`DnsConfigurationResolver`]. Both are modelled,
//! unrenamed, because both are still served.
//!
//! `SplitDns` itself has no struct here: it is a bare map with no properties,
//! so there is nothing for a drift test to check and nothing for a model to
//! hold that [`std::collections::BTreeMap`] does not.

use std::collections::BTreeMap;

use crate::model;
use crate::models::KnownValues;

pub const KNOWN_VALUES: &[KnownValues] = &[];

/// The older split-DNS shape: a domain suffix to the nameservers that answer
/// for it, or to `null` to remove the suffix.
pub type SplitDns = BTreeMap<String, Option<Vec<String>>>;

model! {
    /// The tailnet's global nameservers.
    ///
    /// Sent to set them and returned when reading them; the same shape both
    /// ways, which is why one struct serves three of the description's places.
    DnsNameservers as "GET /tailnet/{tailnet}/dns/nameservers 200" {
        /// Addresses, not URLs. Replacing this with an empty list removes
        /// every global nameserver, which also turns MagicDNS off.
        dns: "dns" => Vec<String>,
    }

    /// The same list, as the request that replaces it.
    DnsNameserversRequest as "POST /tailnet/{tailnet}/dns/nameservers body" is DnsNameservers;

    /// What replacing the nameservers answers with: the new list, and the
    /// state MagicDNS was left in.
    DnsNameserversSet as "POST /tailnet/{tailnet}/dns/nameservers 200" {
        dns: "dns" => Vec<String>,
        /// Turns itself off when the last nameserver goes, which is why the
        /// answer says so rather than leaving a caller to read it back.
        magic_dns: "magicDNS" => bool,
    }

    /// Whether MagicDNS is on.
    DnsPreferences {
        /// Turning this on requires at least one global nameserver.
        magic_dns: "magicDNS" => bool,
    }

    /// The search domains appended to a bare name.
    DnsSearchPaths {
        search_paths: "searchPaths" => Vec<String>,
    }

    /// One nameserver, and whether it survives an exit node.
    DnsConfigurationResolver {
        /// An address of either family, not a URL.
        address: "address" => String,
        /// Keep using this resolver while the device is on an exit node.
        /// Needs Tailscale 1.88.1 or later on the device.
        use_with_exit_node: "useWithExitNode" => bool,
    }

    /// MagicDNS, and whether the tailnet's nameservers replace the machine's.
    DnsConfigurationPreferences {
        /// `true` makes `nameservers` the resolvers; `false` leaves them as a
        /// fallback behind whatever the OS is configured with.
        override_local_dns: "overrideLocalDNS" => bool,
        /// MagicDNS, which needs a global nameserver to be on.
        magic_dns: "magicDNS" => bool,
    }

    /// The whole DNS configuration in one document.
    DnsConfiguration {
        nameservers: "nameservers" => Vec<DnsConfigurationResolver>,
        /// Domain suffix to the resolvers that answer for it, or to `null` to
        /// remove the suffix. Note the resolvers here are objects, where the
        /// older `SplitDns` shape spells them as addresses.
        split_dns: "splitDNS" => BTreeMap<String, Option<Vec<DnsConfigurationResolver>>>,
        search_paths: "searchPaths" => Vec<String>,
        preferences: "preferences" => DnsConfigurationPreferences,
    }
}
