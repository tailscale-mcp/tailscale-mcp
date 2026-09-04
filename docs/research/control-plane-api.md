<!-- Research note. Inventory of the Tailscale control-plane REST API v2 (OpenAPI schema, scopes, drift vs Go client).
     Produced by a research sub-agent on 2026-09-03 during the design interview; facts were verified against the sources named inside. Not a spec. -->

# Tailscale control-plane API v2 — complete inventory

## 0. Schema files kept (Step 1)

- Original download: `<scratchpad>/tailscale-openapi.yaml` (245,452 bytes). Note: `https://api.tailscale.com/api/v2?outputOpenapiSchema=true` returns **YAML** (`content-type: application/yaml`, ETag `30c73c46215dc9c0336df7989c7242524212bede339584f42f3db996ae0eb505`) despite the request; `jq` fails on it. I converted it with Ruby psych.
- JSON conversion (use this for tooling): `<scratchpad>/tailscale-openapi.json`
- Derived dumps: `.../scratchpad/ops.md` (every operation with params/bodies/responses), `.../scratchpad/schemas.md` (all 43 component schemas), `.../scratchpad/goclient/` (clone of tailscale-client-go-v2 @ e3df9fd, Aug 6 2026), `.../scratchpad/kb-1623.txt` (trust-credentials scope page), `.../scratchpad/kb-oauth-clients.txt`, `.../scratchpad/kb-api.txt`.

Spec stats: `openapi: 3.1.0`, `info.title` "Tailscale API", **`info.version` = `v2`**, server `https://api.tailscale.com/api/v2`, **60 paths, 93 operations, 15 tags, 43 component schemas**. Security: single scheme `bearerAuth` (http/bearer) applied globally (`security: [{bearerAuth: []}]`); **there is no per-operation security/scope metadata** — scopes appear only as free text "OAuth Scope: `x`" inside descriptions. Extension fields present: `x-badges` (Alpha, on the 3 Organizations ops only), `x-displayName` (tags), `x-codeSamples`, `x-enumDescriptions`, `x-additionalPropertiesName`. No `x-scopes`, no deprecation markers, no plan/enterprise markers anywhere in the spec. The spec's own intro says: "The API endpoints documented here are stable unless otherwise noted. However, the OpenAPI spec used to generate this documentation is unstable."

Common component responses (all bodies `{"message": string}`): 400, 403, 404, 409, 429, 500, 501, 502, 504; inline extras: 412 (policy If-Match mismatch), 422 (AWS trust-policy validation failure), 202 (webhook test queued).

## 1. Endpoint tables per resource group (all paths relative to `/api/v2`)

Classification: READ = no state change; WRITE = state change (reversible); DESTRUCTIVE = delete/revoke/irreversible or connectivity-breaking. Scopes are quoted from the spec description unless marked "(kb)" = taken from kb/1623 because the spec text carries none.

### Devices (15)

| Method | Path | operationId — purpose | Scope | Class | Params / body / notes |
|---|---|---|---|---|---|
| GET | /tailnet/{tailnet}/devices | listTailnetDevices — list devices | devices:core:read | READ | `fields=all\|default`; server-side filters `<field>=<value>` (exact match on top-level simple/list props, ANDed, e.g. `isEphemeral=true&tags=tag:prod`); returns `{devices:[Device]}`; no pagination |
| PATCH | /tailnet/{tailnet}/device-attributes | batchUpdateCustomDevicePostureAttributes — batch posture attrs | devices:posture_attributes | WRITE | body required `{nodes:{<nodeId>:{"custom:k": value\|null,...}}, comment}`; JSON Merge Patch, `null` deletes; keys must be `custom:`; 200 body is `null` |
| GET | /device/{deviceId} | getDevice | devices:core:read | READ | deviceId = `nodeId` (preferred) or numeric `id`; `fields` as above |
| DELETE | /device/{deviceId} | deleteDevice | devices:core | DESTRUCTIVE | must be in caller's tailnet; shared-in devices unsupported (501) |
| POST | /device/{deviceId}/expire | expireDeviceKey — expire node key | devices:core | DESTRUCTIVE (forces re-auth) | no body |
| GET | /device/{deviceId}/routes | listDeviceRoutes | devices:routes:read | READ | `{advertisedRoutes[], enabledRoutes[]}` |
| POST | /device/{deviceId}/routes | setDeviceRoutes — replace enabled routes | devices:routes | WRITE | body required `{routes:[cidr]}`; advertised routes cannot be set via API; returns DeviceRoutes |
| POST | /device/{deviceId}/authorized | authorizeDevice | devices:core | WRITE | `{authorized*: bool}` (false revokes) |
| POST | /device/{deviceId}/name | setDeviceName | devices:core | WRITE | `{name*}`; old MagicDNS names stop working |
| POST | /device/{deviceId}/tags | setDeviceTags | devices:core | WRITE | `{tags:["tag:x"]}`; tags must be defined in policy file; credential's own tags constrain what it may assign |
| POST | /device/{deviceId}/key | updateDeviceKey — key expiry toggle | devices:core | WRITE | `{keyExpiryDisabled*: bool}` |
| POST | /device/{deviceId}/ip | setDeviceIp — set IPv4 | devices:core | WRITE (disruptive: breaks existing connections) | `{ipv4*}` from CGNAT range / IP pool |
| GET | /device/{deviceId}/attributes | getDevicePostureAttributes | devices:posture_attributes:read | READ | `{attributes:{k: string\|number\|bool}, expiries:{k: date-time}}` |
| POST | /device/{deviceId}/attributes/{attributeKey} | setCustomDevicePostureAttributes | devices:posture_attributes | WRITE | key `custom:*`, ≤128 chars, letters/digits/underscore/colon, unique case-insensitively, value type fixed on first write; body required `{value: string\|number\|bool, expiry?: date-time, comment?}`; **429 declared** |
| DELETE | /device/{deviceId}/attributes/{attributeKey} | deleteCustomDevicePostureAttributes | devices:posture_attributes | DESTRUCTIVE | `custom:` only; **429 declared** |

### DeviceInvites (6)

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| GET | /device/{deviceId}/device-invites | listDeviceInvites | device_invites:read | READ | `[DeviceInvite]` |
| POST | /device/{deviceId}/device-invites | createDeviceInvites | none stated; **user-owned token only** (not OAuth-client-derived) | WRITE | body `[{multiUse, allowExitNode, email}]` → `[DeviceInvite]` incl. `inviteUrl` |
| GET | /device-invites/{deviceInviteId} | getDeviceInvite | device_invites:read | READ | |
| DELETE | /device-invites/{deviceInviteId} | deleteDeviceInvite | device_invites | DESTRUCTIVE | |
| POST | /device-invites/{deviceInviteId}/resend | resendDeviceInvite | none stated; user-owned token only | WRITE (sends email) | only if created with email; **rate limited 1/min** |
| POST | /device-invites/-/accept | acceptDeviceInvite | none stated; user-owned token only | WRITE | `{invite*: "https://login.tailscale.com/admin/invite/xxx" or "xxx"}` → `{device{id,os,name,fqdn,ipv4,ipv6,includeExitNode}, sharer{id,displayName,loginName,profilePicURL}, acceptedBy{same}}` |

### UserInvites (5)

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| GET | /tailnet/{tailnet}/user-invites | listUserInvites — open invites | none stated | READ | `[UserInvite]` |
| POST | /tailnet/{tailnet}/user-invites | createUserInvites | **user-owned keys only** | WRITE | body `[{role: member\|admin\|it-admin\|network-admin\|billing-admin\|auditor, email?}]` → `[UserInvite]` with `inviteUrl` |
| GET | /user-invites/{userInviteId} | getUserInvite | none stated | READ | |
| DELETE | /user-invites/{userInviteId} | deleteUserInvite | user-owned keys only | DESTRUCTIVE | |
| POST | /user-invites/{userInviteId}/resend | resendUserInvite | user-owned keys only | WRITE (email) | only if created with email; **rate limited 1/min** |

### Logging (8)

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| GET | /tailnet/{tailnet}/logging/configuration | listConfigurationAuditLogs | logs:configuration:read | READ | `start`*, `end`* RFC 3339; `actor[]` (IDs or `~search` on login/display name), `target[]` (substring), `event[]` (enum of ~150 codes: ADMIN_CONSOLE.*, API_KEY.CREATE/EXPIRED/REVOKE, BILLING.*, INVITE.*, NODE.* (CREATE, DELETE, APPROVE, UPDATE.ACL_TAGS, UPDATE.MACHINE_NAME, ...), SHARE.*, TAILNET.* (UPDATE.ACL, UPDATE.DNS_CONFIG, ENABLE/DISABLE.* features, TKA), USER.* (APPROVE, SUSPEND, DELETE, UPDATE.USER_ROLE, ...), WEBHOOK_ENDPOINT.*, WEB_INTERFACE.*, PAM_*) → `{version, tailnet, logs:[ConfigurationAuditLog]}` |
| GET | /tailnet/{tailnet}/logging/network | listNetworkFlowLogs | logs:network:read | READ | `start`*, `end`* → `{logs:[NetworkFlowLog]}`; 502 possible |
| GET | /tailnet/{tailnet}/logging/{logType}/stream/status | getLogStreamingStatus | log_streaming:read | READ | logType ∈ `configuration\|network` → LogstreamEndpointPublishingStatus |
| GET | /tailnet/{tailnet}/logging/{logType}/stream | getLogStreamingConfiguration | log_streaming:read | READ | → LogstreamEndpointConfiguration |
| PUT | /tailnet/{tailnet}/logging/{logType}/stream | setLogStreamingConfiguration | log_streaming; **plus device_invites and policy_file for private endpoints** | WRITE | body LogstreamEndpointConfiguration (see schemas); example `{"destinationType":"elastic","url":"http://100.71.134.73:80/...","user":"u","token":"t"}` |
| DELETE | /tailnet/{tailnet}/logging/{logType}/stream | disableLogStreaming | log_streaming | DESTRUCTIVE | |
| POST | /tailnet/{tailnet}/aws-external-id | getAwsExternalId — create-or-get | log_streaming | WRITE (idempotent create) | `{reusable?: bool}` → `{externalId, tailscaleAwsAccountId}` |
| POST | /tailnet/{tailnet}/aws-external-id/{id}/validate-aws-trust-policy | validateAwsExternalId | log_streaming | READ (validation only) | `{roleArn}`; 200 = ok, 422 `{message}` = failed |

### DNS (11) — spec descriptions carry no scope; scopes below are from kb/1623

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| GET | /tailnet/{tailnet}/dns/nameservers | listDnsNameservers | dns:read (kb) | READ | `{dns:[]}` |
| POST | /tailnet/{tailnet}/dns/nameservers | setDnsNameservers — replace list | dns (kb) | WRITE (full replace) | `{dns:[]}` → `{dns[], magicDNS}`; removing all disables MagicDNS |
| GET | /tailnet/{tailnet}/dns/preferences | getDnsPreferences | dns:read (kb) | READ | `{magicDNS}` |
| POST | /tailnet/{tailnet}/dns/preferences | setDnsPreferences | dns (kb) | WRITE | `{magicDNS*: bool}`; errors if no nameservers |
| GET | /tailnet/{tailnet}/dns/searchpaths | listDnsSearchPaths | dns:read (kb) | READ | `{searchPaths[]}` |
| POST | /tailnet/{tailnet}/dns/searchpaths | setDnsSearchPaths — replace | dns (kb) | WRITE (full replace) | `{searchPaths*: []}` |
| GET | /tailnet/{tailnet}/dns/split-dns | getSplitDns | dns:read (kb) | READ | map `{<domain>: [nameserver] \| null}` |
| PATCH | /tailnet/{tailnet}/dns/split-dns | updateSplitDns — partial | dns (kb) | WRITE | only listed domains touched; `null` clears a domain |
| PUT | /tailnet/{tailnet}/dns/split-dns | setSplitDns — replace | dns (kb) | WRITE (full replace) | `{}` clears everything |
| GET | /tailnet/{tailnet}/dns/configuration | getDnsConfiguration — everything at once | not in kb table (assume dns:read) | READ | DnsConfiguration |
| POST | /tailnet/{tailnet}/dns/configuration | setDnsConfiguration — replace everything | not in kb table (assume dns) | WRITE (full replace) | `{nameservers:[{address, useWithExitNode}], splitDNS:{}, searchPaths:[], preferences:{overrideLocalDNS (default false), magicDNS (default false)}}`; `useWithExitNode` needs client v1.88.1+ |

### Keys (5)

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| GET | /tailnet/{tailnet}/keys | listTailnetKeys — auth keys, API tokens, trust credentials | api_access_tokens:read / auth_keys:read / oauth_keys:read / federated_keys:read (each unlocks its key type) | READ | query `all` (boolean; spec marks it required but text treats it as optional). Without `all=true`: user token → only that user's keys; OAuth-derived token → all OAuth clients; federated → all federated identities. Returns `{keys:[Key]}` (secret `key` never included). Go client always sends `all=true`. |
| POST | /tailnet/{tailnet}/keys | createKey — "Create an auth key or trust credential" | auth_keys (keyType auth) / oauth_keys (client) / federated_keys (federated) | WRITE (mints a secret) | body `{keyType: auth\|client\|federated, description (≤50 alnum, hyphens/spaces), capabilities:{devices:{create:{reusable, ephemeral, preauthorized, tags[]}}}, expirySeconds (int64, auth keys only), scopes[], tags[] (mandatory if scopes include devices:core or auth_keys), issuer (uri), subject, audience, customClaimRules{k:v}}` (last 4 federated only). Response Key includes the one-time `key` secret. Key owned by the token's user, or by the tailnet when made with an OAuth/federated-derived token. |
| GET | /tailnet/{tailnet}/keys/{keyId} | getKey | same `*:read` scopes per type; **any scope may GET the key currently in use** | READ | revoked/expired → `invalid: true` |
| DELETE | /tailnet/{tailnet}/keys/{keyId} | deleteKey — revoke | api_access_tokens / auth_keys / oauth_keys / federated_keys | DESTRUCTIVE | |
| PUT | /tailnet/{tailnet}/keys/{keyId} | setKey — reconfigure OAuth client / federated identity | oauth_keys / federated_keys | WRITE | `{keyType: client\|federated, description, scopes[], tags[], issuer, subject, audience, customClaimRules}`; not applicable to auth keys / API tokens |

Note: `keyType` enum on responses is `auth | client | api | federated`; `api` (personal API access tokens) cannot be created via this endpoint (enum on create is auth/client/federated), only listed/read/deleted.

### PolicyFile (4)

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| GET | /tailnet/{tailnet}/acl | getPolicyFile | policy_file:read (credential must also hold devices:posture_attributes:read + devices:core:read) | READ | `Accept: application/json` → JSON; otherwise HuJSON (`application/hujson`). Response header `ETag`. `?details=true` → `{acl: base64 HuJSON, warnings[], errors[]}` (do not send Accept with it) |
| POST | /tailnet/{tailnet}/acl | setPolicyFile — replace whole policy | policy_file (credential must also hold devices:posture_attributes + devices:core:read) | WRITE (full replace, high impact) | body: JSON object (`application/json`) or HuJSON string (`application/hujson`); header `If-Match: "<etag>"` (from GET) or `If-Match: ts-default` (only replace if policy is still the default); **412** on mismatch; 400 for invalid ACL / failing `tests`; `Accept` selects response format |
| POST | /tailnet/{tailnet}/acl/preview | previewRuleMatches | policy_file:read (stated inside the `type` param description) | READ (non-mutating POST) | query `type`* ∈ `user\|ipport`, `previewFor`* (user email, or `10.0.0.1:80`); body = candidate policy (JSON or HuJSON); → `{matches:[{users, ports, lineNumber}], type, previewFor}`; nothing saved |
| POST | /tailnet/{tailnet}/acl/validate | validateAndTestPolicyFile | policy_file:read | READ (non-mutating POST) | Two modes: JSON **array** of tests `[{src, srcPostureAttrs, proto, accept[], deny[]}]` runs them against the current policy; JSON **object** / HuJSON string = hypothetical policy (validated, its `tests` executed). 200 `{message, data[]}`; empty body = passed |

### DevicePosture integrations (5)

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| GET | /tailnet/{tailnet}/posture/integrations | getPostureIntegrations | feature_settings:read | READ | `{integrations:[PostureIntegration]}` |
| POST | /tailnet/{tailnet}/posture/integrations | createPostureIntegration | feature_settings | WRITE | body PostureIntegration; `provider`* ∈ `falcon\|intune\|jamfpro\|kandji\|kolide\|sentinelone` and `clientSecret`* required; `cloudId`, `clientId`, `tenantId`; one integration per provider (409) |
| GET | /posture/integrations/{id} | getPostureIntegration | feature_settings:read | READ | |
| PATCH | /posture/integrations/{id} | updatePostureIntegration | feature_settings | WRITE | `cloudId, clientId, tenantId, clientSecret` (omit to keep); `provider` ignored |
| DELETE | /posture/integrations/{id} | deletePostureIntegration | feature_settings | DESTRUCTIVE | |

### Users (7)

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| GET | /tailnet/{tailnet}/users | listUsers | users:read | READ | query `type` ∈ `member\|shared\|all`, `role` ∈ `owner\|member\|admin\|it-admin\|network-admin\|billing-admin\|auditor\|all` → `{users:[User]}` |
| GET | /users/{userId} | getUser | users:read | READ | |
| POST | /users/{userId}/role | updateUserRole | users | WRITE | `{role: owner\|member\|admin\|it-admin\|network-admin\|billing-admin\|auditor}`; user tokens cannot change their own role |
| POST | /users/{userId}/approve | approveUser | users | WRITE | no body; no-op if approval disabled/already approved; not self |
| POST | /users/{userId}/suspend | suspendUser | users | WRITE (reversible) | not self |
| POST | /users/{userId}/restore | restoreUser | users | WRITE | not self |
| POST | /users/{userId}/delete | deleteUser | users | DESTRUCTIVE | not self |

### Contacts (3)

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| GET | /tailnet/{tailnet}/contacts | getContacts | account_settings:read | READ | `{account, support, security}` each `{email, fallbackEmail, needsVerification}` |
| PATCH | /tailnet/{tailnet}/contacts/{contactType} | updateContact | account_settings | WRITE | contactType ∈ `account\|support\|security`; `{email*}`; email change triggers verification mail |
| POST | /tailnet/{tailnet}/contacts/{contactType}/resend-verification-email | resendContactVerificationEmail | account_settings | WRITE (email) | only while verification pending |

### Webhooks (7)

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| GET | /tailnet/{tailnet}/webhooks | listWebhooks | webhooks:read | READ | `{webhooks:[Webhook]}` |
| POST | /tailnet/{tailnet}/webhooks | createWebhook | webhooks | WRITE (secret returned once) | `{endpointUrl*, providerType?: slack\|mattermost\|googlechat\|discord, subscriptions*: [enum below]}` |
| GET | /webhooks/{endpointId} | getWebhook | webhooks:read | READ | |
| PATCH | /webhooks/{endpointId} | updateWebhook | webhooks | WRITE | `{subscriptions[]}` only |
| DELETE | /webhooks/{endpointId} | deleteWebhook | webhooks | DESTRUCTIVE | |
| POST | /webhooks/{endpointId}/test | testWebhook | webhooks | WRITE (side effect only, no state change) | **202**; async event of type `test` |
| POST | /webhooks/{endpointId}/rotate | rotateWebhookSecret | webhooks | DESTRUCTIVE (old secret invalidated) | returns Webhook with new `secret` (used for `Tailscale-Webhook-Signature`) |

Subscriptions enum: `nodeCreated, nodeNeedsApproval, nodeApproved, nodeKeyExpiringInOneDay, nodeKeyExpired, nodeDeleted, nodeSigned, nodeNeedsSignature, policyUpdate, userCreated, userNeedsApproval, userSuspended, userRestored, userDeleted, userApproved, userRoleUpdated, subnetIPForwardingNotEnabled, exitNodeIPForwardingNotEnabled`.

### TailnetSettings (2)

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| GET | /tailnet/{tailnet}/settings | getTailnetSettings | feature_settings:read (general); logs:network:read (`networkFlowLoggingOn`); networking_settings:read (`httpsCertificates` per text — schema field is `httpsEnabled`); policy_file:read (`aclsExternallyManagedOn`, `aclsExternalLink`) | READ | TailnetSettings |
| PATCH | /tailnet/{tailnet}/settings | updateTailnetSettings | feature_settings / logs:network / networking_settings / policy_file, per field | WRITE | partial TailnetSettings body |

### Services (7)

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| GET | /tailnet/{tailnet}/services | listServices | services:read | READ | `{vipServices:[VIPServiceInfo]}` |
| GET | /tailnet/{tailnet}/services/{serviceName} | getService | services:read | READ | serviceName is `svc:<name>`, unique tailnet-wide (cannot collide with machine names) |
| PUT | /tailnet/{tailnet}/services/{serviceName} | updateService — upsert | services | WRITE (create or update) | body required VIPServiceInfoPut `{name, displayName (≤64), addrs[], comment, ports ["tcp:80", ... or "do-not-validate"], tags[]}`; on create body `name` must equal path; on update body `name` renames |
| DELETE | /tailnet/{tailnet}/services/{serviceName} | deleteService | services | DESTRUCTIVE | |
| GET | /tailnet/{tailnet}/services/{serviceName}/devices | listServiceHosts | services **and** devices:core | READ | `{hosts:[{stableNodeID, approvalLevel: not-approved\|approved:auto\|approved:manual, configured}]}` |
| GET | /tailnet/{tailnet}/services/{serviceName}/device/{deviceId}/approved | getServiceDeviceApproval | services and devices:core | READ | `{approved, autoApproved}` |
| POST | /tailnet/{tailnet}/services/{serviceName}/device/{deviceId}/approved | updateServiceDeviceApproval | services and devices:core | WRITE | body required `{approved: bool}` |

### OAuthApps (5)

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| GET | /tailnet/{tailnet}/oauth-apps | listOAuthApps | oauth_apps:read | READ | `{oauthApps:[OAuthApp]}` |
| POST | /tailnet/{tailnet}/oauth-apps | createOAuthApp | oauth_apps (+ devices:posture_attributes if `allowedNodeAttributes` given) | WRITE (clientSecret returned once) | `{name* (3–50 chars, alnum - . _), description (≤300), redirectURIs* (https, or http localhost/127.0.0.1/::1), scopes* (e.g. "auth_keys:create"), allowedNodeAttributes (custom:*)}` |
| GET | /tailnet/{tailnet}/oauth-apps/{appId} | getOAuthApp | oauth_apps:read | READ | |
| PUT | /tailnet/{tailnet}/oauth-apps/{appId} | updateOAuthApp | oauth_apps | WRITE | same body; secret not regenerated nor returned |
| DELETE | /tailnet/{tailnet}/oauth-apps/{appId} | deleteOAuthApp | oauth_apps | DESTRUCTIVE | |

### Organizations (3) — every op carries `x-badges: Alpha`

| Method | Path | operationId — purpose | Scope | Class | Notes |
|---|---|---|---|---|---|
| DELETE | /tailnet/{tailnet} | deleteTailnet | all | DESTRUCTIVE (tailnet + all users/devices/config) | API-only tailnets; use a token for that tailnet (exchange the OAuth client returned at creation, or an `all`-scoped client of the creating tailnet) |
| GET | /organizations/{organization}/tailnets | listOrganizationTailnets | tailnets:read | READ | `organization` = `-` for the token's org; `limit` (default and max 100), `cursor` → `{tailnets:[{id, displayName, orgId, createdAt}], cursor, totalCount}` — **the only paginated endpoint** |
| POST | /organizations/{organization}/tailnets | createOrganizationTailnet | tailnets | WRITE (mints an OAuth client secret, returned once) | body required `{displayName*}` → `{id, displayName, orgId, dnsName, createdAt, oauthClient{id, secret}, alreadyExists}`; API-only tailnets have no human users and are invisible in the admin console; max 10 tailnets per plan incl. the original |

Endpoints known to exist but **absent from the OpenAPI paths** (for completeness of an MCP wrapper):

| Method | Path | Purpose | Source |
|---|---|---|---|
| POST | /oauth/token | OAuth 2.0 client_credentials → 1h access token | kb/1215, Go `oauth.go` |
| POST | /oauth/token-exchange | federated identity: form `client_id` + `jwt` (OIDC ID token) → same token response shape | Go `identityfederation.go` (not documented in KB captures) |

## 2. Component schemas (43) — full field lists

- **Device**: `addresses[]`, `id` (legacy numeric string), `nodeId` (preferred), `user`, `name`, `hostname`, `clientVersion`, `updateAvailable`, `os`, `created` (date-time), `connectedToControl`, `lastSeen`, `keyExpiryDisabled`, `expires`, `authorized`, `isExternal`, `multipleConnections`, `machineKey`, `nodeKey`, `blocksIncomingConnections`, `enabledRoutes[]`, `advertisedRoutes[]`, `clientConnectivity{endpoints[], mappingVariesByDestIP, latency{<derpRegion>:{preferred, latencyMs}}, clientSupports{hairPinning (always null), ipv6, pcp, pmp, udp, upnp}}`, `tags[]`, `tailnetLockError`, `tailnetLockKey`, `sshEnabled`, `postureIdentity{serialNumbers[], disabled}`, `isEphemeral`, `distro{name, version, codeName}`. `fields=default` subset: addresses, id, nodeId, user, name, hostname, clientVersion, updateAvailable, os, created, connectedToControl, lastSeen, keyExpiryDisabled, expires, authorized, isExternal, machineKey, nodeKey, blocksIncomingConnections, tailnetLockKey, tailnetLockError (schema notes add tags, isEphemeral).
- **DeviceRoutes**: `advertisedRoutes[]`, `enabledRoutes[]`.
- **DevicePostureAttributes**: `attributes{k: string|number|boolean}`, `expiries{k: date-time}`.
- **DeviceInvite**: `id`, `created`, `tailnetId` (int64), `deviceId` (int64), `sharerId` (int64), `multiUse`, `allowExitNode`, `email`, `lastEmailSentAt`, `inviteUrl`, `accepted`, `acceptedBy{id, loginName, profilePicUrl}`.
- **UserInvite**: `id*`, `role*` (member|admin|it-admin|network-admin|billing-admin|auditor), `tailnetId*`, `inviterId*`, `email`, `lastEmailSentAt`, `inviteUrl`.
- **Key**: `id`, `key` (creation only), `keyType` (auth|client|api|federated), `expirySeconds` (auth only), `created`, `updated`, `expires`, `revoked`, `capabilities` → **KeyCapabilities** `{devices:{create:{reusable, ephemeral, preauthorized, tags[]}}}`, `scopes[]`, `tags[]`, `description`, `invalid`, `userId`, `audience`, `issuer` (uri), `subject`, `customClaimRules{k:v}` (federated only).
- **User**: `id`, `displayName`, `loginName`, `profilePicUrl`, `tailnetId`, `created`, `type` (member|shared), `role` (owner|member|admin|it-admin|network-admin|billing-admin|auditor), `status` (active|idle|suspended|needs-approval|over-billing-limit), `deviceCount`, `lastSeen`, `currentlyConnected`.
- **Contact**: `email`, `fallbackEmail`, `needsVerification`; contactType enum account|support|security.
- **Webhook**: `endpointId`, `endpointUrl`, `providerType` (**providerType** schema: slack|mattermost|googlechat|discord or ""), `creatorLoginName`, `created`, `lastModified`, `subscriptions` (**subscriptions** schema, enum above), `secret` (password; create/rotate only).
- **DnsPreferences** `{magicDNS*}`; **DnsSearchPaths** `{searchPaths*[]}`; **SplitDns** map `<domain> → string[] | null`; **DnsConfigurationResolver** `{address, useWithExitNode}`; **DnsConfigurationPreferences** `{overrideLocalDNS, magicDNS}`; **DnsConfiguration** `{nameservers:[Resolver], splitDNS, searchPaths[], preferences}`.
- **PostureIntegration**: `provider` (falcon|intune|jamfpro|kandji|kolide|sentinelone; required on create, ignored on update), `cloudId`, `clientId`, `tenantId`, `clientSecret` (required on create), `id` (readOnly), `configUpdated`, `status{lastSync, error, providerHostCount, matchedCount, possibleMatchedCount}`.
- **TailnetSettings**: `aclsExternallyManagedOn`, `aclsExternalLink` (uri), `devicesApprovalOn`, `devicesAutoUpdatesOn`, `devicesKeyDurationDays` (int), `usersApprovalOn`, `usersRoleAllowedToJoinExternalTailnets` (none|admin|member), `networkFlowLoggingOn`, `regionalRoutingOn`, `postureIdentityCollectionOn`, `httpsEnabled` (booleans nullable).
- **LogType** enum configuration|network. **LogstreamEndpointConfiguration**: `logType`, `destinationType` (splunk|elastic|panther|cribl|crowdstrike|datadog|axiom|s3), `url`, `user`, `uploadPeriodMinutes`, `compressionFormat` (zstd|gzip|none), `token`, `s3Bucket`, `s3Region`, `s3KeyPrefix`, `s3AuthenticationType` (accesskey|rolearn), `s3AccessKeyId`, `s3SecretAccessKey`, `s3RoleArn`, `s3ExternalId` (readOnly), `gcsBucket`, `gcsKeyPrefix`, `gcsScopes[]`, `gcsCredentials`. **LogstreamEndpointPublishingStatus**: `lastActivity, lastError, maxBodySize, numBytesSent, numEntriesSent, numSpoofedEntries, numTotalRequests, numFailedRequests, rateBytesSent, rateEntriesSent, rateTotalRequests, rateFailedRequests` (all required). **AwsExternalId** `{externalId, tailscaleAwsAccountId}`.
- **ConfigurationAuditLog**: `eventTime`, `type` (=CONFIG), `deferredAt` (time the audit-log rate limiter enqueued the record), `eventGroupID`, `origin` (enum), `actor{id, type, loginName, displayName, tags}`, `target{id, name, type, isEphemeral, property}`, `action` (enum), `old`, `new`, `actionDetails`, `error`. **NetworkFlowLog**: `logged`, `nodeId`, `start`, `end`, `virtualTraffic[]`, `subnetTraffic[]`, `exitTraffic[]`, `physicalTraffic[]` of **ConnectionCounts** `{proto, src, dst, txPkts, txBytes, rxPkts, rxBytes}`.
- **VIPServiceInfo** `{name (svc:), displayName (≤64), addrs[], comment, ports[], tags[]}`; **VIPServiceInfoPut** = allOf(`{addrs[]}`, VIPServiceInfo); **ServiceHostInfo** `{stableNodeID, approvalLevel, configured}`; **VIPServiceApproval** `{approved, autoApproved}`.
- **OAuthApp**: `id`, `name`, `description`, `redirectURIs[]`, `scopes[]`, `allowedNodeAttributes[]`, `clientSecret` (creation only), `created`, `updated` (plus standalone schemas **name**, **description**, **redirectURIs**, **scopes**, **allowedNodeAttributes**).
- **OrganizationTailnet** `{id, displayName, orgId, createdAt}`; **ListOrganizationTailnetsResponse** `{tailnets[], cursor, totalCount}`; **CreateOrganizationTailnetRequest** `{displayName*}`; **TailnetOAuthClient** `{id, secret}`; **CreateOrganizationTailnetResponse** `{id, displayName, orgId, dnsName, createdAt, oauthClient, alreadyExists}`.
- **Error** `{message*}`.
- Component parameters: tailnet, fields, deviceId, attributeKey, userInviteId, deviceInviteId, start, end, actor, target, event, logType, all, keyId, AcceptHeaderParam, id, userId, contactType, endpointId, serviceName, appId, organization.

## 3. Auth summary

- Base URL `https://api.tailscale.com/api/v2/`. Token goes in `Authorization: Bearer <token>` or as the HTTP Basic username with an empty password (`curl -u "tskey-api-xxx:"`).
- **API access tokens** (`tskey-api-` prefix): created only in the admin console Keys page (not creatable via `createKey`); expiry 1–90 days inclusive; same permissions as the owning user; case-sensitive; revocable; listable/deletable via `/keys` with `api_access_tokens(:read)`. Some endpoints require a user-owned token (user/device invite create/delete/resend/accept); user tokens cannot act on their own user (role/approve/suspend/restore/delete).
- **Trust credentials** = OAuth clients (secret `tskey-client-`) and federated OIDC identities; they never expire, are not tied to a user, and mint scoped short-lived tokens. Owners/Admins can create clients with any scope/tag; other roles only within their own permissions (e.g. Network admin cannot grant `devices:core`, IT admin can). Revoking a credential revokes all tokens it issued; token creation is audit-logged with the client ID as actor.
- **OAuth token endpoint**: `POST https://api.tailscale.com/api/v2/oauth/token`, OAuth 2.0 client_credentials, form body `client_id`, `client_secret`, optional `scope` and `tags` (space-delimited; must be a subset of the client's grants; `tags` is non-standard and only meaningful with `devices:core`, `auth_keys`, or `all`). Response `{"access_token":"tskey-...","token_type":"Bearer","expires_in":3600,"scope":"..."}`. Lifetime is 1 hour and "cannot be modified" — cache and refresh shortly before expiry. An OAuth client secret can also be used directly as an auth key (`tailscale up --auth-key='tskey-client-...?ephemeral=false&preauthorized=true' --advertise-tags=tag:ci`).
- **Federated identities**: `POST /api/v2/oauth/token-exchange` with form `client_id` and `jwt` (per Go client), same response shape.
- **`tailnet` path param**: `-` = default tailnet of the token (recommended); otherwise the tailnet ID from General Settings (preferred); legacy IDs (e.g. `example.com`) still work for tailnets created before Oct 2025. **`organization`**: `-` = the token's organization.
- Every scope also allows `GET /tailnet/:tailnet/keys/:keyID` for the key in use; only `all` / `all:read` can list all tokens in the tailnet.

## 4. Complete scope list (kb/1623 "Trust credentials", last validated Jan 30, 2026; granular scopes since Nov 14, 2024)

| Scope | Unlocks |
|---|---|
| all:read | every `*:read` endpoint incl. future ones; GET keys/:keyID for any key |
| all | every endpoint incl. future ones; GET/DELETE keys/:keyID for any key; required for DELETE /tailnet/{tailnet} |
| dns:read | GET dns/nameservers, dns/preferences, dns/searchpaths, dns/split-dns |
| dns | dns:read + POST nameservers/preferences/searchpaths, PATCH/PUT split-dns |
| policy_file:read | GET acl, POST acl/preview, POST acl/validate (credential must also carry devices:posture_attributes:read and devices:core:read) |
| policy_file | policy_file:read + POST acl (must also carry devices:posture_attributes and devices:core:read) |
| users:read | GET tailnet/users, GET user by id |
| users | users:read + POST role/approve/suspend/restore/delete |
| devices:core:read | GET tailnet/devices, GET device |
| devices:core | devices:core:read + DELETE device, POST authorized/expire/ip/name/key/tags. **Tags mandatory on the credential**; auth keys it creates must use those tags or tags they own |
| devices:posture_attributes:read | GET device/attributes (kb also lists GET attributes/:key, which the spec does not have) |
| devices:posture_attributes | read + POST/DELETE device/attributes/:attributeKey (kb also lists POST/DELETE on /attributes without key, not in spec); spec adds PATCH tailnet/device-attributes and OAuth-app allowedNodeAttributes |
| devices:routes:read | GET device/routes |
| devices:routes | read + POST device/routes |
| device_invites:read (kb table spells `devices_invites:read`) | GET device/device-invites, GET device-invites/:id |
| device_invites (kb: `devices_invites`) | read + DELETE device-invites/:id (create/resend/accept need a user-owned token) |
| api_access_tokens:read | GET keys and keys/:id for API access tokens |
| api_access_tokens | read + DELETE keys/:id for API access tokens |
| auth_keys:read | GET keys and keys/:id for auth keys |
| auth_keys | read + POST keys (auth), DELETE keys/:id. **Tags mandatory on the credential** |
| oauth_keys:read | GET keys/:id for OAuth clients/tokens (spec: also list) |
| oauth_keys | read + DELETE keys/:id; spec: POST keys (client), PUT keys/:id |
| federated_keys:read | GET keys/:id for federated identities and their keys (spec: also list) |
| federated_keys | read + DELETE; spec: POST keys (federated), PUT keys/:id |
| webhooks:read | GET tailnet/webhooks, GET webhooks/:id |
| webhooks | read + POST create, PATCH/DELETE, POST test, POST rotate |
| log_streaming:read | GET logging/:logType/stream and .../stream/status |
| log_streaming | read + PUT/DELETE stream; spec adds POST aws-external-id and validate-aws-trust-policy; private endpoints additionally need device_invites + policy_file |
| logs:configuration:read | GET logging/configuration |
| logs:network:read | GET logging/network; GET tailnet/settings |
| logs:network | read + PATCH tailnet/settings (network logging field only) |
| account_settings:read | GET contacts |
| account_settings | read + PATCH contacts/:type, POST resend-verification-email |
| feature_settings:read | GET posture/integrations, GET posture/integrations/:id, GET tailnet/settings |
| feature_settings | read + POST posture/integrations, PATCH/DELETE posture/integrations/:id, PATCH tailnet/settings |

Scopes referenced by the spec but **not present in the kb/1623 table** (unverified beyond the spec text): `services:read`, `services` (Services), `oauth_apps:read`, `oauth_apps` (OAuth apps), `tailnets:read`, `tailnets` (Organizations), `networking_settings:read`, `networking_settings` (settings `httpsCertificates`/`httpsEnabled`). OAuth *apps* use a different scope vocabulary (example `auth_keys:create`) whose full list I could not find.

Legacy scopes (pre Nov 14, 2024; existing credentials remain valid): `all`, `all:read`, `acl`, `acl:read`, `devices`, `devices:read`, `dns`, `dns:read`, `routes`, `routes:read`, `logs:read`, `network-logs:read`. Mapping: `devices` → `auth_keys` + `devices:posture_attributes` + `devices:core`; `routes` → `devices:routes`; `acl` → `policy_file`; `logs` → `logs:configuration`; `network-logs` → `logs:network`.

## 5. Rate-limit facts

- No numeric rate limits are published anywhere: not in the spec, not in kb/1101 or kb/1215/1623. `https://tailscale.com/kb/1225/` is the "fast user switching" page, not rate limits. Web searches surfaced only Aperture (a separate Tailscale gateway product) 429/Retry-After docs, which do not apply to api.tailscale.com.
- Documented limits: "Invite resends are rate limited to one per minute" (user-invite and device-invite resend).
- The 429 component response is declared only on `POST` and `DELETE /device/{deviceId}/attributes/{attributeKey}`.
- ConfigurationAuditLog `deferredAt` reveals a server-side audit-log rate limiter (internal, not client-facing).
- OAuth tokens live exactly 1 hour; mint one per hour, not per request.
- Recommendation for the MCP server: treat 429 generically (honor `Retry-After` if present, exponential backoff), and mark the two attribute endpoints and invite resends explicitly.

## 6. Counts

Total **93 operations across 60 paths** (plus 2 undocumented OAuth token endpoints). Per group: Devices 15, DNS 11, Logging 8, Users 7, Webhooks 7, Services 7, DeviceInvites 6, UserInvites 5, Keys 5, DevicePosture 5, OAuthApps 5, PolicyFile 4, Contacts 3, Organizations 3, TailnetSettings 2. By class: READ 39 (incl. 3 non-mutating POSTs: acl/preview, acl/validate, validate-aws-trust-policy), WRITE 36, DESTRUCTIVE 18.

## 7. Go client (tailscale.com/client/tailscale/v2) cross-check

Resources and methods (12 resources, covering 63 of 93 ops): Contacts{Get, Update}; DevicePosture{ListIntegrations, GetIntegration, CreateIntegration, UpdateIntegration, DeleteIntegration}; Devices{List, ListWithAllFields, Get, GetWithAllFields, Delete, SubnetRoutes, SetSubnetRoutes, SetAuthorized, SetName, SetTags, SetKey, SetIPv4Address, GetPostureAttributes, SetPostureAttribute, DeletePostureAttribute}; DNS{Nameservers, SetNameservers, Preferences, SetPreferences, SearchPaths, SetSearchPaths, SplitDNS, UpdateSplitDNS, SetSplitDNS, Configuration, SetConfiguration}; Keys{List, Get, Create, CreateAuthKey, CreateOAuthClient, CreateFederatedIdentity, SetOAuthClient, SetFederatedIdentity, Delete}; Logging{GetNetworkFlowLogs, LogstreamConfiguration, SetLogstreamConfiguration, DeleteLogstreamConfiguration, CreateOrGetAwsExternalId, ValidateAWSTrustPolicy}; PolicyFile{Get, Raw, Set, SetAndGet, Validate}; Services{List, Get, CreateOrUpdate, Delete}; TailnetSettings{Get, Update}; Tailnets{Create, List, Delete}; Users{List, Get}; Webhooks{List, Create, Get, Update, Delete, Test, RotateSecret}.

In OpenAPI but **missing from the Go client** (30 ops): POST device/expire; PATCH tailnet/device-attributes; all 6 DeviceInvites; all 5 UserInvites; all 5 OAuthApps; GET logging/configuration (audit logs); GET logging/{logType}/stream/status; POST acl/preview; users role/approve/suspend/restore/delete; contacts resend-verification-email; services /devices and /device/{id}/approved GET+POST.

In the Go client but **not in OpenAPI**: `POST /api/v2/oauth/token`, `POST /api/v2/oauth/token-exchange`; the Services resource calls `/tailnet/{tailnet}/vip-services` whereas the spec documents `/tailnet/{tailnet}/services` (both may be live; unverified); Go `Service` struct has an `annotations` map absent from the spec. Enum drift: Go posture providers add `fleet`, `huntress`; Go logstream destinations add `gcs` and lack `crowdstrike` (spec has gcs* fields but no `gcs` enum value); Go webhook subscriptions lack `nodeSigned`/`nodeNeedsSignature` and add `categoryTailnetManagement`/`categoryDeviceMisconfigurations`.

## 8. Ambiguities / unverified items (clearly marked)

1. The "official" schema is served as YAML, not JSON; the JSON file is my conversion.
2. Scopes are not machine-readable; 21 ops carry no "OAuth Scope" line (all 11 DNS ops, 5 UserInvites, 3 DeviceInvites create/resend/accept, plus acl/preview whose scope sits in a parameter description). DNS scopes were filled from kb/1623; `dns/configuration` is not in the kb table at all.
3. Eight spec scopes (`services`, `oauth_apps`, `tailnets`, `networking_settings` and their `:read` forms) are not in the trust-credentials doc; whether they can be selected on an OAuth client today is unverified.
4. Doc/spec path mismatches: kb uses `/user/:userID` (spec `/users/{userId}`), `/logging/:logType/status` (spec `.../stream/status`), lists `GET /device/:id/attributes/:key` and `POST,DELETE /device/:id/attributes` (not in spec), spells `devices_invites` vs `device_invites`, and "logstreaming:read". Treat the spec as authoritative.
5. TailnetSettings text says `httpsCertificates` is gated by `networking_settings`, but the schema field is `httpsEnabled`.
6. `listTailnetKeys` marks `all` required while describing it as optional.
7. Plan gating: the spec has zero enterprise/premium markers. Features such as network flow logs, log streaming, posture integrations, user approval, and Services are plan-dependent in Tailscale's pricing, but that could not be verified from the API sources — flag per tool at implementation time.
8. Organizations endpoints are Alpha and limited to 10 tailnets per plan; API-only tailnets only.
9. No pagination except organizations list; device lists on large tailnets are returned whole.
10. `https://tailscale.com/api` is JS-rendered; its index sections could not be fetched, so the 15 spec tags stand in for them.
11. `/vip-services` vs `/services` and the `/oauth/token-exchange` request format come from Go client source only.
