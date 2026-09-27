# Provider Proxy Settings Design

## Goal

Let users enter one HTTP or HTTPS proxy URL in General settings and choose independently which providers use it for Quota01's provider API requests.

## Confirmed behavior

- The proxy URL is an optional app-wide setting.
- Each provider has a separate `useProxy` setting, disabled by default so existing installations continue to connect directly.
- A provider uses the configured proxy only when its `useProxy` setting is enabled and the proxy URL is non-empty. Otherwise it connects directly.
- Only HTTP and HTTPS proxy URLs are in scope. SOCKS is out of scope because the current `reqwest` dependency does not enable SOCKS support.
- The proxy policy applies to provider HTTP requests made by Quota01. It does not change local CLI reads, local provider processes, WebView sign-in traffic, update downloads, or pricing catalog downloads.
- Invalid proxy URLs must produce a clear settings validation error. Proxy URLs and any embedded credentials must not be written to logs.
- A saved URL or provider toggle takes effect on the provider's next refresh; the user does not need to restart Quota01.

## Current project context

Settings are represented by `AppSettings` and `ProviderLayout` in `src-tauri/src/models.rs`, mirrored by TypeScript interfaces in `src/lib/types.ts`, and normalized/persisted by `src-tauri/src/settings.rs`. General settings are edited in `src/lib/SettingsScreen.svelte`; provider-specific settings are edited in `src/lib/CustomizeProviderDetail.svelte`. Settings copy lives in `src/lib/i18n/messages/*.ts`.

Provider runtimes are constructed once in `src-tauri/src/providers/registry.rs`. Their HTTP clients are mostly constructed independently in each provider's `client.rs` and held for the lifetime of the runtime. The provider service calls each runtime from `src-tauri/src/service.rs`. This means a proxy implementation must pass current, provider-specific network policy into the request/client layer rather than only changing settings UI or relying on process-wide proxy environment variables.

## Design

Add `proxy_url: Option<String>` to `AppSettings` and `use_proxy: bool` to `ProviderLayout`, with serde defaults for existing saved settings. Mirror both fields in the frontend types. Normalize the URL by trimming whitespace and treating an empty value as unset. Bump the settings schema version from 9 to 10. Keep each provider's toggle on its existing provider customization screen; put the URL input in the General settings section. The URL input keeps its local edit draft until blur or Enter so intermediate text while typing is not submitted as an invalid setting.

At refresh time, `ProviderService` resolves the effective network policy from the latest `AppSettings` and the target provider's `use_proxy` value. Pass that policy through an explicit provider refresh context. Add a shared provider HTTP client builder that applies the HTTP(S) proxy only when that context says to use it. Update provider HTTP clients to use the shared builder and current policy, retaining client reuse for a provider while its policy is unchanged. The update path must not require rebuilding the application or silently preserve an old proxy configuration after settings change.

Provider requests that use local data or local processes remain direct and do not need an HTTP proxy. Provider implementations that combine local and remote requests must apply the policy only to the remote HTTP client. The updater and pricing clients remain independent from provider proxy settings.

## Validation and failure behavior

- Accept blank input as no proxy.
- Accept only absolute `http://` or `https://` proxy URLs with a host and valid port, if present.
- Reject other schemes, missing hosts, malformed URLs, and invalid ports when settings are saved, with a user-readable error.
- If a valid proxy cannot connect, the normal provider refresh failure path reports a network error for that provider; other providers continue using their own configured routes.
- Do not log the raw proxy URL, username, or password in settings, client-builder, or request-failure diagnostics.

## UI and localization

Add a labeled URL text input and short help text in General settings. Add a labeled checkbox to each provider's customization detail. Clearly indicate that the provider checkbox uses the shared URL. Add matching strings to all currently supported locale dictionaries in `src/lib/i18n/messages` so the locale type and dictionary contract remain complete.

## Compatibility

Adding serde-defaulted settings preserves older persisted settings. Existing providers default to direct connections. Bump the settings schema version from 9 to 10, matching the version normalization writes for all settings.

## Alternatives considered

1. **Shared provider HTTP client construction with explicit per-refresh policy (selected):** supports independent routing and applying settings on the next refresh, while keeping proxy logic and validation centralized.
2. **Rebuild all provider runtimes after every proxy setting change:** keeps proxy configuration in constructors, but introduces runtime replacement and in-flight refresh lifecycle concerns.
3. **Set process-wide proxy environment variables:** does not provide per-provider routing and can affect unrelated application traffic.

## Acceptance criteria

1. A user can save or clear an HTTP(S) proxy URL in General settings.
2. A user can independently enable or disable proxy use for every provider.
3. Existing settings load with proxy disabled for every provider and no configured proxy URL.
4. A provider with proxy enabled uses the configured proxy on its next HTTP refresh; a provider with it disabled connects directly.
5. Invalid proxy input is rejected clearly, and proxy credentials never appear in logs.
6. Non-provider traffic and local provider data flows remain unaffected.
7. All supported locale dictionaries include the new settings labels.
