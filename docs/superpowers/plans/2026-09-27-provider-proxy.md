# Provider Proxy Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add one HTTP(S) proxy URL in General settings and let users independently choose whether each provider's HTTP requests use it.

**Architecture:** Persist the URL in `AppSettings` and the per-provider choice in `ProviderLayout`. Resolve a current `ProviderRequestContext` for each provider refresh and pass it into a shared proxy-aware client factory, so settings changes apply on the next request while a provider can reuse a client for an unchanged policy. Keep app-wide pricing and update clients, WebView traffic, local provider processes, and local Antigravity HTTP traffic outside the provider proxy policy.

**Tech Stack:** Rust, Tauri 2, reqwest 0.13 blocking clients, serde, Svelte 5, TypeScript, Vitest, Rust unit tests.

**Spec:** `docs/superpowers/specs/2026-09-27-provider-proxy-design.md`

## Global Constraints

- Proxy schemes are absolute `http://` or `https://` URLs with a host and a valid optional port.
- Blank proxy URL means no proxy; a provider uses the proxy only when `useProxy` is true and a URL is configured.
- Every provider's `useProxy` defaults to false, preserving direct connections for existing settings.
- Provider settings changes apply on the next provider refresh without restarting Quota01.
- Proxy URL credentials must never appear in logs.
- Provider WebView traffic, local provider processes, Antigravity loopback HTTP, pricing downloads, and update downloads remain unaffected.
- Update settings schema version from 9 to 10; serde defaults load existing settings as direct connections.
- Add the new labels to all 16 current locale dictionaries under `src/lib/i18n/messages`.

## Review Focus

- An older settings document omits `proxyUrl` and every `useProxy`: load no proxy and direct connections. Test in Task 1's serde compatibility test.
- The proxy URL is blank or whitespace-only: normalize it to no proxy. Test in Task 1's normalization test.
- The proxy URL uses an unsupported scheme, has no host, or has an invalid port: reject saving it with a user-readable settings error. Test in Task 1's URL validation tests.
- A provider toggle is on while the shared URL is unset: send that provider directly. Test in Task 2's policy resolution test.
- A configured proxy fails or contains credentials: isolate the network error to that provider and keep the URL and credentials out of logs. Test in Tasks 2 and 3.

---

### Task 1: Persist and validate proxy settings

**Files:**
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/settings.rs`
- Modify: `src-tauri/src/commands/settings.rs` (provider layout test fixtures)
- Modify: `src-tauri/src/pacing.rs` (provider layout test fixtures)
- Modify: `src-tauri/src/providers/codex/mapper.rs` (provider layout test fixtures)
- Test: `src-tauri/src/settings.rs`

**Interfaces:**
- Produces: `AppSettings.proxy_url: Option<String>` and `ProviderLayout.use_proxy: bool`, both with serde defaults; schema version 10.
- Produces: `pub(crate) fn normalize_proxy_url(value: Option<String>) -> Result<Option<String>, String>` which trims whitespace, maps empty values to `None`, and accepts only absolute HTTP(S) URLs with a host and valid port.

- [ ] **Step 1: Add a settings compatibility test**

Add a test in the existing `settings.rs` test module that deserializes a pre-feature JSON object with no `proxyUrl` and no `useProxy` fields, then asserts `proxy_url == None` and every provider `use_proxy == false`.

```rust
let saved = serde_json::json!({ "schemaVersion": 9, "providers": [
    { "id": "codex", "enabled": true, "detected": true, "expanded": false,
      "keychainAccessGranted": false, "metrics": [] }
]});
let settings: AppSettings = serde_json::from_value(saved).unwrap();
assert_eq!(settings.proxy_url, None);
assert!(!settings.providers[0].use_proxy);
```

- [ ] **Step 2: Run the focused test and confirm it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml settings::tests::settings_without_proxy_fields_default_to_direct -- --exact`
Expected: FAIL because the new settings fields do not yet exist.

- [ ] **Step 3: Add fields and normalize URL values**

Add serde-defaulted fields to `AppSettings` and `ProviderLayout`, set `AppSettings::default().schema_version` and `normalize_with_persisted_accounts` to 10, set new provider layouts' `use_proxy` to false, and implement `normalize_proxy_url` using `reqwest::Url::parse`. Call it from `SettingsService::update_locked` before normalization and persistence so invalid values return its user-readable error through the existing settings save command.

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub proxy_url: Option<String>,

#[serde(default)]
pub use_proxy: bool,
```

- [ ] **Step 4: Test normalization and validation cases**

Add tests for `None`, empty and whitespace-only values, a valid `http://127.0.0.1:8080`, a valid HTTPS URL, `socks5://`, a hostless URL, malformed input, and an out-of-range port. Assert accepted values are trimmed and invalid values return an error.

```rust
assert_eq!(
    normalize_proxy_url(Some("  http://127.0.0.1:8080  ".into())).unwrap(),
    Some("http://127.0.0.1:8080".into())
);
assert_eq!(normalize_proxy_url(Some("   ".into())).unwrap(), None);
assert!(normalize_proxy_url(Some("socks5://127.0.0.1:1080".into())).is_err());
```

- [ ] **Step 5: Run settings tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml settings::tests`
Expected: PASS, including existing schema normalization tests updated to expect version 10.

- [ ] **Step 6: Commit the settings model**

```bash
git add src-tauri/src/models.rs src-tauri/src/settings.rs
git add src-tauri/src/commands/settings.rs src-tauri/src/pacing.rs src-tauri/src/providers/codex/mapper.rs
git commit -m "feat: persist provider proxy settings"
```

### Task 2: Resolve current provider request policy

**Files:**
- Modify: `src-tauri/src/providers/mod.rs`
- Modify: `src-tauri/src/service.rs`
- Modify: `src-tauri/src/providers/workbuddy/mod.rs`
- Modify: `src-tauri/src/providers/codex/mod.rs`
- Test: `src-tauri/src/service.rs`
- Test: `src-tauri/src/providers/mod.rs`

**Interfaces:**
- Produces: `ProviderRequestContext { pub proxy_url: Option<reqwest::Url> }` with `ProviderRequestContext::direct()`.
- Produces: `ProviderService::request_context_for(&self, provider_id: &str) -> ProviderRequestContext`, which resolves the current settings and returns direct policy if the toggle or URL is absent.
- Produces: `pub(crate) fn resolve_proxy_url(value: Option<String>, use_proxy: bool) -> Result<Option<reqwest::Url>, String>`; this normalizes any configured URL through Task 1, then returns `None` when the toggle is disabled or the URL is empty.
- Produces: `UsageProvider::refresh_with_context(&self, context: &ProviderRequestContext) -> Result<ProviderSnapshot, ProviderError>`; its default implementation calls existing `refresh()` so local/test providers remain direct until migrated.
- Produces: `UsageProvider::refresh_for_service_with_context(&self, context: &ProviderRequestContext) -> Result<ProviderRefresh, ProviderError>`; default behavior preserves current cache identity behavior and calls `refresh_with_context`.
- Consumes: Task 1's `normalize_proxy_url` and latest `AppSettings`.

- [ ] **Step 1: Add a service policy-resolution test**

Add `provider_proxy_policy_is_resolved_per_provider` to resolve all four cases: no URL and toggle off, URL and toggle off, URL and toggle on, and no URL with toggle on. Assert only the URL-present/toggle-on case yields `Some(Url)`.

```rust
assert_eq!(resolve_proxy_url(None, false).unwrap(), None);
assert_eq!(resolve_proxy_url(Some("http://127.0.0.1:8080".into()), false).unwrap(), None);
assert_eq!(resolve_proxy_url(Some("http://127.0.0.1:8080".into()), true).unwrap()
    .unwrap().host_str(), Some("127.0.0.1"));
assert_eq!(resolve_proxy_url(None, true).unwrap(), None);
```

- [ ] **Step 2: Run the focused test and confirm it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml provider_proxy_policy_is_resolved_per_provider`
Expected: FAIL because provider request policy is not implemented.

- [ ] **Step 3: Add provider request context and trait defaults**

Add the context and trait methods in `providers/mod.rs`. Preserve the existing no-context `refresh()` and `refresh_for_service()` methods for direct unit-test call sites. Implement `refresh_for_service_with_context` in `CodexProvider` and `WorkBuddyProvider`, preserving their account/cache behavior while passing context into remote HTTP clients.

```rust
fn refresh_with_context(
    &self,
    context: &ProviderRequestContext,
) -> Result<ProviderSnapshot, ProviderError> {
    let _ = context;
    self.refresh()
}

fn refresh_for_service_with_context(
    &self,
    context: &ProviderRequestContext,
) -> Result<ProviderRefresh, ProviderError> {
    let snapshot = self.refresh_with_context(context)?;
    Ok(ProviderRefresh {
        snapshot,
        cache_identity: self.cache_identity().resolved_value().map(str::to_owned),
        account: None,
    })
}
```

- [ ] **Step 4: Resolve the context immediately before each refresh**

In `ProviderService::run_refresh_flight`, read the current settings once per attempt, locate the matching `ProviderLayout`, parse the normalized URL, and call `refresh_for_service_with_context`. If no settings service exists (test constructor) or the toggle/URL is absent, pass `ProviderRequestContext::direct()`.

```rust
let context = self.request_context_for(&provider_id);
worker_provider.refresh_for_service_with_context(&context)
```

- [ ] **Step 5: Verify policy resolution and refresh isolation**

Run: `cargo test --manifest-path src-tauri/Cargo.toml provider_proxy_policy_is_resolved_per_provider`
Expected: PASS; add an assertion that a provider's proxy policy does not alter a second provider's context.

- [ ] **Step 6: Commit request policy plumbing**

```bash
git add src-tauri/src/providers/mod.rs src-tauri/src/service.rs src-tauri/src/providers/workbuddy/mod.rs src-tauri/src/providers/codex/mod.rs
git commit -m "feat: resolve provider proxy policy per refresh"
```

### Task 3: Add a proxy-aware HTTP client factory

**Files:**
- Create: `src-tauri/src/providers/http.rs`
- Modify: `src-tauri/src/providers/mod.rs`
- Modify: `src-tauri/src/service.rs`
- Modify: `src-tauri/src/providers/test_http.rs`
- Test: `src-tauri/src/providers/http.rs`

**Interfaces:**
- Modifies: Extend `ProviderRequestContext` with `pub http_clients: Arc<ProviderHttpClientFactory>`; change `ProviderRequestContext::direct()` to `ProviderRequestContext::direct(http_clients: Arc<ProviderHttpClientFactory>)`; `ProviderService` owns the shared factory and puts a clone in every context.
- Produces: `ProviderHttpClientFactory::client(&self, provider_id: &str, profile: &str, proxy_url: Option<&reqwest::Url>, configure: impl FnOnce(reqwest::blocking::ClientBuilder) -> reqwest::blocking::ClientBuilder) -> Result<Arc<reqwest::blocking::Client>, ProviderHttpClientError>`.
- Produces: test helper `serve_once_capturing_request(status: u16, body: &str) -> (String, std::thread::JoinHandle<String>)`, returning the local URL and captured request headers.
- Produces: test helper `serve_sequence_capturing_requests(responses: &[(u16, &str)]) -> (String, std::thread::JoinHandle<Vec<String>>)` for WorkBuddy's multi-request login flow.
- Produces: redacted `ProviderHttpClientError` variants `ProxyConfiguration { provider_id: String }` and `ClientBuild { provider_id: String }`; neither variant stores nor displays the proxy URL.
- The cache key is `(provider_id, profile, proxy_url)`. `profile` distinguishes clients for the same provider that require different timeouts, TLS behavior, or redirect policy. The configuration closure runs only when building a missing cache entry. When a new proxy URL is cached, evict older entries for the same provider/profile; in-flight `Arc<Client>` clones remain usable and the cache stays bounded.
- Consumes: Task 2's `ProviderRequestContext`; provider code calls `context.http_clients.client(provider_id, profile, context.proxy_url.as_ref(), configure)`.

- [ ] **Step 1: Add client-factory tests**

Add tests named `provider_proxy_client_reuses_unchanged_policy`, `provider_proxy_client_rebuilds_for_changed_url`, and `provider_proxy_client_error_redacts_credentials`. Assert identical keys reuse the same `Arc<Client>`, changed URL or direct/proxy settings return different clients and evict the old cache entry for that provider/profile, and the error display includes the provider id but not a credential-bearing proxy URL.

- [ ] **Step 2: Run the focused tests and confirm they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml provider_proxy_client_`
Expected: FAIL because `providers/http.rs` does not exist.

- [ ] **Step 3: Implement the cache and proxy builder**

Store `Arc<Client>` values in a mutex-protected map keyed by `(provider_id, profile, proxy_url)`. On a cache miss, call the supplied `configure` closure, add `reqwest::Proxy::all(proxy_url.as_str())` when a URL is present, build the client, evict stale entries for the same provider/profile, and cache the new client. Return a redacted error variant that includes the provider id but never formats the proxy URL or credentials.

```rust
let mut builder = configure(reqwest::blocking::Client::builder());
if let Some(url) = proxy_url {
    builder = builder.proxy(reqwest::Proxy::all(url.as_str()).map_err(|_| {
        ProviderHttpClientError::ProxyConfiguration {
            provider_id: provider_id.to_owned(),
        }
    })?);
}
let client = builder.build().map_err(|_| ProviderHttpClientError::ClientBuild {
    provider_id: provider_id.to_owned(),
})?;
```

In `test_http.rs`, implement both capture helpers using the existing `TcpListener` response patterns, returning accepted request text through their join handles so route and proxy-auth tests can inspect headers without logging them.

- [ ] **Step 4: Verify client reuse and redaction**

Run: `cargo test --manifest-path src-tauri/Cargo.toml provider_proxy_client_`
Expected: PASS; assert errors containing a credential-bearing URL do not contain the username or password.

- [ ] **Step 5: Commit the HTTP client factory**

```bash
git add src-tauri/src/providers/http.rs src-tauri/src/providers/mod.rs src-tauri/src/service.rs src-tauri/src/providers/test_http.rs
git commit -m "feat: add provider proxy-aware HTTP client factory"
```

### Task 4: Migrate identity and subscription provider clients

**Files:**
- Modify: `src-tauri/src/providers/antigravity/client.rs`
- Modify: `src-tauri/src/providers/antigravity/mod.rs`
- Modify: `src-tauri/src/providers/claude/client.rs`
- Modify: `src-tauri/src/providers/claude/mod.rs`
- Modify: `src-tauri/src/providers/codex/client.rs`
- Modify: `src-tauri/src/providers/codex/mod.rs`
- Modify: `src-tauri/src/providers/copilot/client.rs`
- Modify: `src-tauri/src/providers/copilot/mod.rs`
- Modify: `src-tauri/src/providers/cursor/client.rs`
- Modify: `src-tauri/src/providers/cursor/mod.rs`
- Test: provider client tests in the listed provider modules

**Interfaces:**
- Consumes: Tasks 2–3's `ProviderRequestContext` and `ProviderHttpClientFactory`.
- Produces: each provider's refresh path obtains its cached client from the factory using a stable provider/profile key and the current context.

- [ ] **Step 1: Add a direct/proxy route test to one representative client per provider**

Use the existing local HTTP test server to add `provider_proxy_identity_route_antigravity`, `provider_proxy_identity_route_claude`, `provider_proxy_identity_route_codex`, `provider_proxy_identity_route_copilot`, and `provider_proxy_identity_route_cursor`. Assert that a remote provider endpoint request reaches a local proxy when configured and reaches its endpoint directly when the context is direct. For Antigravity, assert remote requests use the proxy while `127.0.0.1` language-server requests always use the existing local client.

- [ ] **Step 2: Run the focused route tests and confirm they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml provider_proxy_identity_route_`
Expected: at least the new proxy-route assertions fail before client migration.

- [ ] **Step 3: Thread current context into provider clients**

Update each provider's `refresh_with_context` implementation and HTTP request methods. Use client profiles to preserve existing timeout, user-agent, redirect, and invalid-certificate settings. Keep Antigravity's loopback `local` client out of the factory; route only its remote OAuth and cloud clients through the proxy-aware factory.

```rust
let client = context.http_clients.client(
    "claude",
    "usage",
    context.proxy_url.as_ref(),
    |builder| builder.timeout(Duration::from_secs(15)),
)?;
client.get(&config.usage_url).bearer_auth(token).send()
```

- [ ] **Step 4: Run provider client tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml provider_proxy_identity_route_`
Expected: PASS; existing direct-route fixtures remain direct and proxy-route cases use the configured proxy.

- [ ] **Step 5: Commit the migrated provider clients**

```bash
git add src-tauri/src/providers/antigravity src-tauri/src/providers/claude src-tauri/src/providers/codex src-tauri/src/providers/copilot src-tauri/src/providers/cursor
git commit -m "feat: proxy identity provider requests"
```

### Task 5: Migrate API-key provider clients

**Files:**
- Modify: `src-tauri/src/providers/deepseek/client.rs`
- Modify: `src-tauri/src/providers/deepseek/mod.rs`
- Modify: `src-tauri/src/providers/grok/client.rs`
- Modify: `src-tauri/src/providers/grok/mod.rs`
- Modify: `src-tauri/src/providers/infini/client.rs`
- Modify: `src-tauri/src/providers/infini/mod.rs`
- Modify: `src-tauri/src/providers/kimi/client.rs`
- Modify: `src-tauri/src/providers/kimi/mod.rs`
- Modify: `src-tauri/src/providers/minimax/client.rs`
- Modify: `src-tauri/src/providers/minimax/mod.rs`
- Test: provider client tests in the listed provider modules

**Interfaces:**
- Consumes: Tasks 2–3's `ProviderRequestContext` and `ProviderHttpClientFactory`.
- Produces: all five API-key providers use a cached client keyed by provider, client profile, and effective proxy URL.

- [ ] **Step 1: Add proxy route tests for API-key requests**

For each client, configure an endpoint URL and proxy URL using the existing test HTTP utilities. Add `provider_proxy_api_key_route_deepseek`, `provider_proxy_api_key_route_grok`, `provider_proxy_api_key_route_infini`, `provider_proxy_api_key_route_kimi`, and `provider_proxy_api_key_route_minimax`. Assert proxy-enabled requests arrive at the proxy and the `Proxy-Authorization` header is not replaced by provider bearer/API-key authentication.

```rust
let (proxy_url, request) = crate::providers::test_http::serve_once_capturing_request(
    200,
    r#"{"marker":"proxy"}"#,
);
let proxy_url = proxy_url.replacen("http://", "http://proxy-user:proxy-pass@", 1);
let context = ProviderRequestContext {
    proxy_url: Some(reqwest::Url::parse(&proxy_url).unwrap()),
    http_clients: Arc::new(ProviderHttpClientFactory::default()),
};
let client = DeepSeekClient::for_test(
    "http://provider.invalid/summary",
    "http://provider.invalid/cost",
    Duration::from_secs(1),
);
let response = client.fetch_summary(&context, "provider-api-key").unwrap();
assert_eq!(response.body["marker"], "proxy");
let request = request.join().unwrap().to_ascii_lowercase();
assert!(request.contains("proxy-authorization: basic "));
assert!(request.contains("authorization: bearer provider-api-key"));
```

- [ ] **Step 2: Run the new API-key route tests and confirm they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml provider_proxy_api_key_route_`
Expected: FAIL on each proxy route until that provider uses the factory.

- [ ] **Step 3: Pass request context through API-key provider refreshes**

Update the five provider refresh implementations and client methods. Preserve current per-client timeouts, URL overrides, headers, and retry behavior in the factory configuration closure.

```rust
let client = context.http_clients.client(
    "deepseek",
    "default",
    context.proxy_url.as_ref(),
    |builder| builder
        .connect_timeout(Duration::from_secs(8))
        .timeout(timeout)
        .user_agent(BROWSER_USER_AGENT),
)?;
```

- [ ] **Step 4: Run API-key provider tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml provider_proxy_api_key_route_`
Expected: PASS with both direct and proxy route tests.

- [ ] **Step 5: Commit API-key provider migration**

```bash
git add src-tauri/src/providers/deepseek src-tauri/src/providers/grok src-tauri/src/providers/infini src-tauri/src/providers/kimi src-tauri/src/providers/minimax
git commit -m "feat: proxy API-key provider requests"
```

### Task 6: Migrate remaining HTTP providers and device-code login

**Files:**
- Modify: `src-tauri/src/providers/opencode/client.rs`
- Modify: `src-tauri/src/providers/opencode/mod.rs`
- Modify: `src-tauri/src/providers/openrouter/client.rs`
- Modify: `src-tauri/src/providers/openrouter/mod.rs`
- Modify: `src-tauri/src/providers/siliconflow/client.rs`
- Modify: `src-tauri/src/providers/siliconflow/mod.rs`
- Modify: `src-tauri/src/providers/trae/client.rs`
- Modify: `src-tauri/src/providers/trae/mod.rs`
- Modify: `src-tauri/src/providers/workbuddy/client.rs`
- Modify: `src-tauri/src/providers/workbuddy/login.rs`
- Modify: `src-tauri/src/providers/workbuddy/mod.rs`
- Modify: `src-tauri/src/providers/mod.rs` (context-aware login trait methods)
- Modify: `src-tauri/src/providers/zai/client.rs`
- Modify: `src-tauri/src/providers/zai/mod.rs`
- Modify: `src-tauri/src/commands/provider_login.rs`
- Test: provider client and login tests in the listed provider modules

**Interfaces:**
- Consumes: Tasks 2–3's request context and HTTP client factory.
- Produces: refresh requests and Rust-driven WorkBuddy device-code HTTP requests use the provider's current proxy policy; WebView sign-in remains outside the HTTP client policy.

- [ ] **Step 1: Add proxy route tests for remaining provider and login requests**

Use local HTTP fixtures to cover one usage request for OpenCode, OpenRouter, SiliconFlow, Trae, WorkBuddy and Z.ai, plus WorkBuddy device-code state/token/account requests using `serve_sequence_capturing_requests`. Add usage tests `provider_proxy_remaining_route_opencode`, `provider_proxy_remaining_route_openrouter`, `provider_proxy_remaining_route_siliconflow`, `provider_proxy_remaining_route_trae`, `provider_proxy_remaining_route_workbuddy`, and `provider_proxy_remaining_route_zai`; name the login test `provider_proxy_remaining_route_workbuddy_login`. Verify provider-specific redirects and headers remain unchanged.

- [ ] **Step 2: Run the focused route tests and confirm they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml provider_proxy_remaining_route_`
Expected: new proxy route assertions fail until the remaining clients receive the context.

- [ ] **Step 3: Migrate provider refresh clients and WorkBuddy login**

Add default context-aware login methods to `UsageProvider` that delegate to the existing methods. Override them in `WorkBuddyProvider`; add the current context to both `start_provider_login_inner` and `poll_provider_login_inner` by resolving it with `ProviderService::request_context_for`. Update provider implementations and `DeviceCodeLogin` to request cached clients from the shared factory. Preserve `DeviceCodeLogin`'s no-redirect policy and keep WebView transport separate.

Update `start_provider_login` to receive `State<Arc<ProviderService>>`, clone it into `start_provider_login_inner`, and add `service: Arc<ProviderService>` to that inner function. For polling, pass the existing service argument into `poll_provider_login_inner`.

```rust
fn start_device_code_login_with_context(
    &self,
    context: &ProviderRequestContext,
) -> Result<DeviceCodeChallenge, ProviderError> {
    let _ = context;
    self.start_device_code_login()
}

fn poll_device_code_login_with_context(
    &self,
    login_id: &str,
    context: &ProviderRequestContext,
) -> DeviceCodePoll {
    let _ = context;
    self.poll_device_code_login(login_id)
}
```

- [ ] **Step 4: Run remaining provider and login tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml provider_proxy_remaining_route_`
Expected: PASS for direct and proxy routes; existing login-state, redirect, and credential-boundary tests remain green.

- [ ] **Step 5: Commit remaining HTTP provider migration**

```bash
git add src-tauri/src/providers/opencode src-tauri/src/providers/openrouter src-tauri/src/providers/siliconflow src-tauri/src/providers/trae src-tauri/src/providers/workbuddy src-tauri/src/providers/zai src-tauri/src/commands/provider_login.rs
git add src-tauri/src/providers/mod.rs
git commit -m "feat: proxy remaining provider requests"
```

### Task 7: Add General and per-provider proxy controls

**Files:**
- Modify: `src/lib/SettingsScreen.svelte`
- Modify: `src/lib/CustomizeProviderDetail.svelte`
- Modify: `src/lib/types.ts`
- Modify: `src/test/appFixtures.ts`
- Modify: `src/lib/reorderComponents.test.ts` (provider layout fixture)
- Modify: `src/lib/shareCard.test.ts` (provider layout fixtures)
- Modify: `src/lib/settingsController.test.ts` (schema-version fixture)
- Modify: `src/lib/CustomizeProviderDetail.session.test.ts` (provider layout fixture)
- Test: `src/lib/SettingsScreen.test.ts`
- Test: `src/lib/CustomizeProviderDetail.session.test.ts`

**Interfaces:**
- Consumes: backend fields `proxyUrl` and `ProviderLayout.useProxy`.
- Produces: a General settings text field whose draft saves on blur or Enter; a checkbox in each provider detail that updates only that provider's `useProxy` value.

- [ ] **Step 1: Add UI behavior tests**

In the settings screen tests, assert the General settings URL field displays the saved URL and commits a trimmed value on blur/Enter. Assert an invalid value is sent to the save path and the component retains its edit draft while the parent still supplies the last saved settings; Task 1's backend test asserts the validation error. In provider detail tests, assert toggling one provider preserves every other provider field and proxy setting.

```ts
await fireEvent.input(screen.getByRole('textbox', { name: 'Proxy URL' }), {
  target: { value: ' http://127.0.0.1:8080 ' },
});
await fireEvent.blur(screen.getByRole('textbox', { name: 'Proxy URL' }));
expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ proxyUrl: 'http://127.0.0.1:8080' }));
```

In `CustomizeProviderDetail.session.test.ts`, click the proxy checkbox for `deepseek` and assert only that provider's layout changes:

```ts
expect(onChange).toHaveBeenLastCalledWith(
  expect.objectContaining({
    providers: expect.arrayContaining([
      expect.objectContaining({ id: 'deepseek', useProxy: true }),
      expect.objectContaining({ id: 'trae-cn', useProxy: false }),
    ]),
  }),
);
```

- [ ] **Step 2: Run focused UI tests and confirm they fail**

Run: `corepack pnpm exec vitest run src/lib/SettingsScreen.test.ts src/lib/CustomizeProviderDetail.session.test.ts`
Expected: FAIL because proxy controls do not exist yet.

- [ ] **Step 3: Implement both controls**

Add `proxyUrl: string | null` and `useProxy: boolean` to `src/lib/types.ts`. Add the General text field with a component-local draft and blur/Enter save behavior. Add a provider-detail checkbox with a label and helper text explaining that it uses the shared proxy URL.

```svelte
<input
  type="url"
  bind:value={proxyUrlDraft}
  onblur={commitProxyUrl}
  onkeydown={(event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      commitProxyUrl();
    }
  }}
/>
```

- [ ] **Step 4: Run focused UI tests**

Run: `corepack pnpm exec vitest run src/lib/SettingsScreen.test.ts src/lib/CustomizeProviderDetail.session.test.ts`
Expected: PASS; editing one provider's toggle does not modify other providers' settings.

- [ ] **Step 5: Commit proxy controls**

```bash
git add src/lib/SettingsScreen.svelte src/lib/CustomizeProviderDetail.svelte src/lib/types.ts src/lib/SettingsScreen.test.ts src/lib/CustomizeProviderDetail.session.test.ts
git add src/test/appFixtures.ts src/lib/reorderComponents.test.ts src/lib/shareCard.test.ts src/lib/settingsController.test.ts
git commit -m "feat: add provider proxy controls"
```

### Task 8: Localize labels and verify full contracts

**Files:**
- Modify: `src/lib/i18n/messages/en.ts`
- Modify: `src/lib/i18n/messages/zh-CN.ts`
- Modify: `src/lib/i18n/messages/zh-TW.ts`
- Modify: `src/lib/i18n/messages/ar.ts`
- Modify: `src/lib/i18n/messages/de.ts`
- Modify: `src/lib/i18n/messages/es.ts`
- Modify: `src/lib/i18n/messages/fr.ts`
- Modify: `src/lib/i18n/messages/hi.ts`
- Modify: `src/lib/i18n/messages/it.ts`
- Modify: `src/lib/i18n/messages/ja.ts`
- Modify: `src/lib/i18n/messages/ko.ts`
- Modify: `src/lib/i18n/messages/pl.ts`
- Modify: `src/lib/i18n/messages/pt-BR.ts`
- Modify: `src/lib/i18n/messages/ru.ts`
- Modify: `src/lib/i18n/messages/tr.ts`
- Modify: `src/lib/i18n/messages/vi.ts`
- Test: `src/lib/i18n.test.ts`

**Interfaces:**
- Produces: matching translation keys `settings.proxyUrl`, `settings.proxyUrlHelp`, `customize.useProxy`, and `customize.useProxyHelp` in all 16 current locale dictionaries.

- [ ] **Step 1: Add locale completeness assertions**

Extend the i18n test to import each locale dictionary and assert all 16 expose the four proxy labels; also assert the English and Simplified Chinese values are non-empty.

```ts
for (const messages of [en, zhCn, zhTw, ar, de, es, fr, hi, it, ja, ko, pl, ptBr, ru, tr, vi]) {
  expect(messages.settings.proxyUrl).toBeTruthy();
  expect(messages.settings.proxyUrlHelp).toBeTruthy();
  expect(messages.customize.useProxy).toBeTruthy();
  expect(messages.customize.useProxyHelp).toBeTruthy();
}
```

- [ ] **Step 2: Run the locale test and confirm it fails**

Run: `corepack pnpm exec vitest run src/lib/i18n.test.ts`
Expected: FAIL because the proxy translation keys are missing.

- [ ] **Step 3: Add all translation values**

Add concise translations for the shared URL label/help and per-provider toggle/help in each dictionary. Keep identical key names and nesting in all 16 files.

```ts
settings: {
  proxyUrl: 'Proxy URL',
  proxyUrlHelp: 'HTTP or HTTPS proxy shared by providers.',
},
customize: {
  useProxy: 'Use proxy',
  useProxyHelp: "Send this provider's requests through the shared proxy URL.",
},
```

- [ ] **Step 4: Run locale and project verification**

Run: `corepack pnpm verify`
Expected: PASS; versions, contracts, frontend lint/check/tests/build, Rust formatting/clippy/tests, and project-wide contracts agree.

- [ ] **Step 5: Commit localization and final verification fixes**

```bash
git add src/lib/i18n/messages/en.ts src/lib/i18n/messages/zh-CN.ts src/lib/i18n/messages/zh-TW.ts src/lib/i18n/messages/ar.ts src/lib/i18n/messages/de.ts src/lib/i18n/messages/es.ts src/lib/i18n/messages/fr.ts src/lib/i18n/messages/hi.ts src/lib/i18n/messages/it.ts src/lib/i18n/messages/ja.ts src/lib/i18n/messages/ko.ts src/lib/i18n/messages/pl.ts src/lib/i18n/messages/pt-BR.ts src/lib/i18n/messages/ru.ts src/lib/i18n/messages/tr.ts src/lib/i18n/messages/vi.ts src/lib/i18n.test.ts
git commit -m "feat: localize provider proxy settings"
```
