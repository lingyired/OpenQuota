# WorkBuddy Provider Integration Design

## Goal

Add a first-class `workbuddy` provider to OpenQuota using the local CodeBuddy/WorkBuddy desktop credentials and the billing APIs described in `workbuddy-credits-integration.md`.

The provider must expose WorkBuddy's credit-based quota and official request usage without treating credits as tokens. Existing Codex, Cursor, OpenRouter, and token-based providers must keep their current behavior and cached data compatibility.

## Existing primitives to reuse

OpenQuota already has two suitable representations for credit balances:

- `ValueMetric` / `MetricValueKind::Count` for a remaining credit balance and optional labeled values.
- `QuotaWindow` with `QuotaFormat::Count` for a total/used/remaining credit window.

The implementation will follow the existing provider patterns:

- Codex for local auth, token refresh, atomic credential persistence, and credit value metrics.
- Cursor for authenticated REST requests, optional usage history, and partial endpoint handling.
- OpenRouter for presenting a credit quota, balance, and period totals as separate metrics.

The existing `UsageHistory` model is token-specific (`tokens`, `totalTokens`, and token-only trend labels). A small unit-aware extension is required only for detailed WorkBuddy request history. Existing providers will continue to produce token usage through the compatibility path.

## Provider definition and metrics

Register `workbuddy` in the provider registry and expose a provider definition with:

- Provider id: `workbuddy`.
- Display name: `WorkBuddy`.
- Links for the WorkBuddy dashboard/profile and status page when the project link conventions require them.
- A quota metric for the current credit package/aggregate balance.
- A value metric for remaining credits, including expiry information when available.
- Period metrics for Today, Yesterday, and Last 30 Days using credit units.
- A credit trend metric using the existing usage-trend surface.

The provider should be detected when a valid local credential file contains an access token. A file that exists but lacks an access token is not a detected credential.

## Credential loading and host selection

Implement a dedicated `workbuddy/auth.rs` module. It reads the platform-specific file:

- macOS: `~/Library/Application Support/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info`
- Windows: `%LOCALAPPDATA%/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info`
- Linux: `~/.local/share/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info`

The parser must preserve the original JSON tree and support both camelCase and snake_case fields. Field precedence is:

- Access token: `auth.accessToken`, `auth.access_token`, root `accessToken`, root `access_token`.
- Refresh token: corresponding camelCase/snake_case variants.
- Token type: corresponding field, defaulting to `Bearer`.
- Domain: root `domain`, then `auth.domain`.
- User id: root `uid`, `account.uid`, `account.id`.
- Nickname: root `nickname`, root `name`, `account.nickname`, `account.label`.
- Email: root `email`, `account.email`, `auth.email`.
- Expiration: root `expiresAt`, then `auth.expiresAt`, with snake_case alternatives.

Host selection is intentionally narrow:

- `www.workbuddy.cn` and `workbuddy.cn` map to `https://www.workbuddy.cn`.
- Every other domain maps to `https://www.codebuddy.cn`.

The account domain remains the value sent in `X-Domain`; request `Origin` and `Referer` use the selected request host. The implementation must not allow an arbitrary credential domain to become an unrestricted request destination.

## HTTP client and authentication behavior

Implement a dedicated `workbuddy/client.rs` with test-server injection similar to the existing providers.

All billing requests use:

- `Authorization: <token_type> <access_token>`.
- `Content-Type: application/json`.
- `Accept: application/json, text/plain, */*`.
- `X-User-Id` when a uid is available.
- `X-Domain` using the account domain.
- `X-Enterprise-Id` and `X-Tenant-Id` when an enterprise id is available.
- `X-Client-Platform: web`.
- Host-derived `Origin` and `Referer` (`/profile/plans-usage`).
- A stable browser-like user agent.

The three current package endpoints are requested in parallel:

```text
POST /billing/meter/get-user-resource-summary
POST /billing/meter/get-user-resource-paid-packages
POST /billing/meter/get-user-resource-free-packages
```

The request body follows the API contract and must not include user prompts or arbitrary local content.

Endpoint classification must inspect both top-level `code` and `data.code`:

- Success: code `0`, code `200`, or no code with structurally valid data.
- Authentication failure: HTTP `401`/`403`, or an error message indicating an expired/invalid token or authorization.
- WAF response: code `10085`; do not attempt token refresh.
- Code `-1` with a message: retry the same endpoint once.
- Other failures: preserve the endpoint-specific warning and allow independent successful endpoints to be used.

For one refresh operation, at most one token refresh is allowed even when several parallel endpoints report unauthorized. Only the previously unauthorized endpoint branches are retried after a successful refresh. The old endpoint

```text
POST https://www.codebuddy.cn/v2/billing/meter/get-user-resource
```

may be used only when all new package/summary endpoints fail, and it must reuse the already refreshed account state.

## Token refresh and credential persistence

Refresh with:

```text
POST <selected-host>/v2/plugin/auth/token/refresh
X-Refresh-Token: <refresh_token>
```

On success, update the in-memory access/refresh token state and persist the new values back to the original JSON file. Persistence must:

- Preserve unknown fields and account arrays.
- Write through a temporary file and atomic rename.
- Use restrictive file permissions where supported.
- Avoid logging tokens or complete credential JSON.
- Return a credential-storage warning/error if persistence fails rather than silently claiming the refreshed token is durable.

The provider remains read-oriented: it never changes account, package, or usage data. Credential persistence is limited to the token refresh required to keep the read operation working.

## Credit package mapping

Package parsing must not depend on hard-coded package codes. It will use multi-level fallbacks for both `Accounts` and `Packages`:

```text
data.Accounts
data.data.Accounts
data.Response.Data.Accounts
data.data.Response.Data.Accounts
data.accounts
data.data.accounts
```

The same paths apply with `Packages` in place of `Accounts`; empty arrays are valid successful results.

Field candidates include:

- Total: `CycleCapacitySizePrecise`, `CycleCapacitySize`, `CycleTotalCapacity`, `CapacitySizePrecise`, `CapacitySize`, `SlicePeriodCapacitySizePrecise`, `SlicePeriodCapacitySize`.
- Remaining: corresponding `Remain` fields.
- Used: corresponding `Used` fields.
- Expiry: `DeductionEndTime`, `ExpiredTime`, `CycleEndTime`, including case variants.
- Status: `Status` / `status`.
- Name: `PackageName` / `packageName`.
- Code: `PackageCode` / `packageCode`.

Numbers must accept JSON numbers and numeric strings. Timestamps must accept seconds, milliseconds, RFC3339, and the common date-string formats used by the service. If a package item has no direct totals, inspect `SlicePeriodUsageDetails[0]` before treating it as unmappable.

Merge behavior:

- Paid/free package details take precedence over summary rows when they describe the same balance, especially when they provide an expiry time.
- Summary data remains a fallback when paid/free detail is unavailable.
- Multiple active package rows are aggregated without allowing negative used/remaining values.
- Package status and expiry are retained for UI warning/tooltip behavior.

## Official request usage

Detailed request usage always uses the fixed WorkBuddy host, independent of the account domain:

```text
POST https://www.workbuddy.cn/billing/meter/get-user-request-usage
```

The request body is:

```json
{
  "startTime": "<today minus 30 days at 00:00:00>",
  "endTime": "<today at 23:59:59>",
  "pageNum": 1,
  "pageSize": 3000
}
```

Use the project's existing local-date convention for the daily trend. Fetch pages `1..100`, continuing while the page is non-empty and the accumulated sanitized rows are below the reported total. An empty page before the reported total is reached yields a partial/incomplete result; reaching the safety limit also yields a partial result.

Only these fields may survive the response projection:

```text
accountId, accountName, requestId, credit, model, client, requestTime
```

Fields such as `input`, `inputTrunc`, prompts, and other unlisted fields must be discarded before aggregation, caching, logging, or UI mapping. Discard rows with non-finite or negative credit, an unparsable timestamp, or a timestamp outside the requested range. Deduplicate by `(requestId, requestTime)`.

Keep at most 100 sanitized request records per account for bounded internal diagnostics. The public `ProviderSnapshot` exposes aggregates and model totals, not prompt-bearing payloads.

Aggregate the valid rows into:

- Today: local date equals today.
- Yesterday: local date equals yesterday.
- Last 30 Days: the existing dashboard window convention.
- Daily totals with zero-filled dates.
- Model totals sorted by credit descending, request count descending, then model name ascending.

Every usage result has one of three completeness states:

- `complete`: all required pages were read and rows were validly processed.
- `partial`: some pages or rows were unavailable/invalid, but usable aggregates exist.
- `unavailable`: no usable official rows were obtained.

## Unit-aware usage model

Add a serializable `UsageUnit` with at least `Tokens` and `Credits`. Introduce a normalized usage amount for periods, daily points, and model entries so the UI can format the value with the correct unit without changing the meaning of existing token fields.

Compatibility requirements:

- Existing cached snapshots without a unit continue to deserialize as token usage.
- Existing providers continue to emit token usage and retain current cost-estimation behavior.
- WorkBuddy emits credit usage with `estimatedCostUsd = null` unless the API provides an explicitly supported monetary value; credits must not be converted to dollars using an invented exchange rate.
- The legacy token fields are read as a fallback for old snapshots and are not populated with WorkBuddy credits.

The new unit and completeness data must be propagated to the TypeScript types and used by `UsageMetric.svelte`, `UsageTrend.svelte`, and `ModelUsageDetail.svelte`. Labels such as `tokens` and `credits` come from the existing localization/backend glossary mechanism.

## Error states and caching

Use the existing `ProviderErrorKind` classification:

- Missing/invalid local access token: `authentication`.
- Valid credentials but insufficient account permission: `permission`.
- HTTP throttling: `rateLimited`.
- Transport failures: `network`.
- Malformed response: `invalidResponse`.
- Failed token-file persistence: `credentialStorage`.

Successful partial package/usage results return a snapshot with warnings. A total failure uses the normal provider error path so the existing cache/stale UI behavior applies. No error message, warning, cache record, or telemetry payload may contain access tokens, refresh tokens, prompts, or raw response bodies.

## Files and integration points

Expected implementation areas:

- `src-tauri/src/providers/workbuddy/` for auth, client, mapping, usage aggregation, fixtures, and tests.
- Provider module declarations, registry construction, and detection registration.
- `src-tauri/src/models.rs` and `src/lib/types.ts` for unit-aware usage.
- Usage display/trend/model breakdown components and localization messages.
- Provider icon/catalog data and any provider documentation required by repository conventions.

The implementation must remain isolated to the new worktree branch `codex/workbuddy-provider`; the user's uncommitted changes on `main` are not part of this work.

## Tests and acceptance criteria

Rust tests must cover:

- Platform path and nested credential field precedence.
- Domain-to-host selection and request headers.
- Package list path fallbacks, numeric/date parsing, summary fallback, and paid/free precedence.
- Parallel endpoint partial success.
- Unauthorized refresh deduplication, retry scope, WAF `10085`, and `-1` retry behavior.
- Atomic credential persistence while preserving unknown JSON fields.
- Usage pagination, empty-page/limit partial states, deduplication, range filtering, invalid credit filtering, zero-filled daily aggregation, deterministic model sorting, and prompt-field exclusion.
- Backward-compatible usage deserialization and credit-unit mapping.

Frontend tests must cover:

- Credit labels in value metrics, period metrics, trend charts, and model breakdowns.
- Token providers continuing to render token labels and existing dollar estimates.
- Missing, partial, and unavailable WorkBuddy usage states.
- Provider detection, registry metadata, and snapshot/cache serialization contracts.

Verification should include focused Rust tests, frontend tests and type checking, contract validation, formatting, and a full build/check pass required by the repository scripts.

## Non-goals

- Do not scrape or automate WorkBuddy's UI.
- Do not store or display prompt/input fields from usage responses.
- Do not invent a credit-to-dollar conversion for WorkBuddy.
- Do not change Codex reset-credit redemption behavior.
- Do not replace existing provider-specific usage implementations with a broad unrelated refactor.
- Do not carry the current `main` worktree's unrelated uncommitted dashboard changes into this branch.
