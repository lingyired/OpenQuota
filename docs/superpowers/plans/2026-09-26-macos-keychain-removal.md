# macOS Keychain Removal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove all macOS runtime Keychain access, use only the approved local credential sources for five providers, and store Quota01's vault master key in a protected local file.

**Architecture:** Replace macOS system credential-store access with a file-backed vault key store while leaving Windows Credential Manager and Linux Secret Service implementations intact. Each provider's detector, runtime refresh, token refresh, and special entry points will share one local source policy; Antigravity quota data will come only from a proxy-bypassing loopback RPC client. Errors flow through the existing `ProviderViewState`, which already preserves the last successful snapshot on refresh failure.

**Tech Stack:** Rust 2021, Tauri 2, reqwest 0.13 blocking client, rusqlite, ChaCha20-Poly1305, `tempfile`, TypeScript/Svelte settings UI, Cargo unit tests.

**Spec:** `docs/superpowers/specs/2026-09-26-macos-keychain-removal-design.md`

## Global Constraints

- macOS provider credentials may come only from the provider-local sources listed in the spec; no fallback to Keychain, external credential stores, or CLI credential commands.
- The Quota01 app data directory must be `0700`; the local vault master-key file must be exactly `0600` and contain 32 random bytes.
- Windows Credential Manager and Linux Secret Service behavior remain unchanged.
- Provider errors preserve the existing instance and last successful snapshot; failed refreshes expose an error and stale status.
- Antigravity local RPC uses loopback only and bypasses proxies; public HTTP requests retain reqwest environment proxy behavior.
- Do not add proxy UI, macOS Keychain migration, Claude Desktop token/cookie reading, or provider-token import into Quota01's vault.
- Implementation must run on the newest available Luna model (`gpt-6-luna`). If the active execution task cannot change model, route implementation to Luna before editing code.

## Worktree and Branch Preparation

At execution, first read `git status --short` and preserve any unrelated user changes. Use the `using-git-worktrees` skill to create an isolated worktree from the current branch HEAD, which contains both the approved spec and this plan, on branch `codex/macos-keychain-removal`. Do not reuse the visual companion or another task's worktree. All implementation commits belong on this branch; do not modify `main` directly.

## File Map

- `src-tauri/src/providers/credential_vault.rs`: platform key-store adapters, local macOS key file, safe creation/permission validation, vault reset, and vault tests.
- `src-tauri/src/commands/settings.rs`, `src-tauri/src/lib.rs`, `src/lib/backend.ts`: expose an explicit vault reset operation for the later Settings screen confirmation UI; this phase provides the safe command and state errors without redesigning windows.
- `src-tauri/src/providers/credential_store.rs`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`: remove only macOS Security.framework APIs/dependency; retain Windows/Linux platform APIs.
- `src-tauri/src/providers/{codex,cursor,copilot,antigravity,claude}`: provider-specific credential candidates, detection, refresh write-back, and provider tests.
- `src-tauri/src/providers/{keychain_access.rs,detection.rs,mod.rs}`, `src-tauri/src/settings.rs`, `src-tauri/src/models.rs`: remove provider keychain grants and grant-based detection classification.
- `src-tauri/src/lib.rs`: initialize the vault with the app data directory and surface initialization/reset errors without prompting for Keychain.
- `src-tauri/src/service.rs`, `src-tauri/src/commands/provider.rs`: preserve existing instance/snapshot behavior and map new credential failures into the existing provider error state.
- `src/lib/types.ts`, `src/lib/CustomizeProviderList.svelte`, related Svelte tests: remove the Keychain access-grant setting; leave the broader window redesign for the next phase.
- Provider-local test modules alongside each Rust source file: cover source selection, no-fallback behavior, refresh persistence, and error results without reading real user credentials.

## Phase 1: macOS Keychain Removal

### Task 1: Establish the isolated execution checkout

**Files:** No product files.

- [ ] Confirm the active implementation model is `gpt-6-luna`.
- [ ] Read `git status --short`; do not copy or stage unrelated changes.
- [ ] Use the `using-git-worktrees` skill to create branch `codex/macos-keychain-removal` from the approved spec commit and record the returned worktree path.
- [ ] In the worktree, verify `git status --short` is clean, the spec and plan both exist, and `git rev-parse HEAD` equals the source branch HEAD recorded before worktree creation.

**Acceptance:** Work proceeds only in the clean isolated worktree and with Luna.

### Task 2: Add the local macOS vault master-key backend

**Files:**

- Modify: `src-tauri/src/providers/credential_vault.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/commands/settings.rs`, `src/lib/backend.ts`

**Interfaces:**

- Keep `credential_vault::initialize(path: PathBuf) -> Result<(), String>` as the app initialization entry point.
- Replace macOS `SystemVaultKeyStore` with a file-backed implementation receiving the app data directory and using a stable sibling key file named `credentials.key`.
- Keep the existing Windows/Linux system-backed store implementations and the current encrypted vault payload format.

- [ ] Add focused tests for first key creation, exact `0600` key permissions, `0700` app data directory permissions, 32-byte key length, repeat initialization/read stability, tampered permissions, missing key with an existing vault, and preservation of vault bytes on failure. Tests use `tempdir()` and a deterministic test key store; they must not call real OS credential APIs.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml credential_vault::tests -- --nocapture` and confirm new permission/creation tests fail before implementation.
- [ ] Implement macOS `FileVaultKeyStore`: create app data directory with mode `0700`; create key via same-directory temporary file with mode `0600`; write cryptographically random bytes; `sync_all`; atomically persist; on an `AlreadyExists` race read and validate the winner's file. Reject wrong length, symlink/non-regular key files, and permission bits other than `0600`.
- [ ] Treat an existing `credentials.vault` without a valid `credentials.key` as a typed vault-key failure. Do not generate a replacement key, delete encrypted data, or attempt legacy Keychain migration.
- [ ] Add an explicit vault reset operation that removes the vault data and key only after the caller has requested reset; test that reset followed by initialization creates an empty usable vault.
- [ ] Expose `reset_credential_vault` as a Tauri command returning `Result<(), String>` and register it in `lib.rs`; add `resetCredentialVault(): Promise<void>` in `src/lib/backend.ts`. Keep the confirmation prompt and Settings screen presentation in the later UI plan.
- [ ] Initialize the file-backed key store on macOS and preserve platform-specific stores on Windows/Linux.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml credential_vault::tests` and `cargo check --manifest-path src-tauri/Cargo.toml --target aarch64-apple-darwin`.
- [ ] Commit only this task's files with message `feat: store macOS vault key locally`.

### Task 3: Remove macOS system credential APIs and grant state

**Files:**

- Modify: `src-tauri/src/providers/credential_store.rs`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
- Modify: `src-tauri/src/providers/keychain_access.rs`, `src-tauri/src/providers/detection.rs`, `src-tauri/src/providers/mod.rs`
- Modify: `src-tauri/src/providers/codex/mod.rs`, `src-tauri/src/providers/cursor/mod.rs`, `src-tauri/src/providers/copilot/mod.rs`, `src-tauri/src/providers/antigravity/mod.rs`, `src-tauri/src/providers/claude/mod.rs`
- Modify: `src-tauri/src/settings.rs`, `src-tauri/src/models.rs`, `src-tauri/src/commands/settings.rs`, `src-tauri/src/pacing.rs`, `src-tauri/src/providers/codex/mapper.rs`
- Modify: `src/lib/types.ts`, `src/lib/CustomizeProviderList.svelte`, `src/lib/Dashboard.svelte`
- Modify fixtures: `src/lib/reorderComponents.test.ts`, `src/lib/shareCard.test.ts`, `src/lib/providerTabSummary.test.ts`, `src/lib/ProviderRail.test.ts`, `src/lib/CustomizeProviderDetail.session.test.ts`

**Interfaces:**

- Remove `UsageProvider::accesses_system_keychain()` and `keychain_access_granted` from serialized provider settings and frontend types.
- Detection continues to use `CredentialProbeStatus::{Detected, Absent, Unknown}` for local probe success/failure/timeout, without a grant-based `Unknown` branch.
- Keep `credential_store.rs` implementations compiled for Windows/Linux; macOS vault code must use `FileVaultKeyStore` directly and must not expose macOS external/owned password calls.

- [ ] Add a detection unit test proving that an absent local provider source is classified `Absent` regardless of prior grant state; add frontend type/component tests that no grant control or `keychainAccessGranted` field is emitted.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml providers::detection::tests` and the targeted frontend tests; confirm the old grant behavior is captured before removing it.
- [ ] Remove the macOS `security-framework` dependency and its lockfile-only packages by updating Cargo metadata; retain Windows `windows-sys` credential APIs and Linux `secret-service` APIs.
- [ ] Remove `keychain_access` grant storage, `SettingsService::publish_keychain_access`, provider grant overrides, model serialization field, frontend field, and copy that advertises granting access to another app's Keychain. Preserve ordinary provider enable/disable behavior.
- [ ] Update every current grant-field use and fixture listed above, including Dashboard's provider-disable update, Rust settings/pacing/mapper fixtures, and all TypeScript provider model fixtures.
- [ ] Remove only the macOS credential-store implementation; do not change the Windows and Linux branches. Make any macOS external credential-store call fail closed at compile time rather than silently return a false “not found” result.
- [ ] Run `cargo check --manifest-path src-tauri/Cargo.toml --target aarch64-apple-darwin`, `cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc`, `cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-unknown-linux-gnu`, and targeted frontend tests.
- [ ] Commit only this task's files with message `refactor: remove macOS credential grants`.

### Task 4: Make Codex and Cursor file/database-only

**Files:**

- Modify: `src-tauri/src/providers/codex/auth.rs`, `src-tauri/src/providers/codex/mod.rs`, `src-tauri/src/providers/codex/reset_claim.rs`
- Modify: `src-tauri/src/providers/cursor/auth.rs`, `src-tauri/src/providers/cursor/mod.rs`

**Interfaces:**

- Codex candidates come only from `CodexAuthState::load_file_candidates`; `has_local_credentials`, regular refresh, `reload`, write-back, and reset claim all use that same file source.
- Cursor auth state has only `Sqlite(PathBuf)` as a persisted source; detection, quota refresh, and write-back use the same `state.vscdb` candidate selection.

- [ ] Add Codex tests for file candidate detection, Keychain-only credentials ignored, auth refresh write-back to the same `auth.json`, and reset claim candidate loading using only auth files. Add Cursor tests for SQLite-only detection, Keychain-only state ignored, and refresh write-back to `state.vscdb`.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml providers::codex::auth::tests` and `cargo test --manifest-path src-tauri/Cargo.toml providers::cursor::auth::tests`; verify the tests reject old Keychain sources.
- [ ] Remove Codex `AuthSource::Keychain`, `load_keychain_candidate`, Keychain reload/save helpers, and Keychain existence probe. Keep the existing `CODEX_HOME` and file path behavior.
- [ ] Remove Cursor `CursorAuthSource::Keychain`, `load_keychain_auth`, Keychain account enumeration, existence probes, and Keychain write-back. Preserve SQLite selection rules and write refresh results to the originating DB.
- [ ] Ensure reset-credit claim uses file-only Codex candidates even if invoked directly through its command.
- [ ] Run both module test commands plus `cargo check --manifest-path src-tauri/Cargo.toml --target aarch64-apple-darwin`.
- [ ] Commit only this task's files with message `fix: use local Codex and Cursor credentials only`.

### Task 5: Make Copilot editor/config-file-only

**Files:**

- Modify: `src-tauri/src/providers/copilot/auth.rs`, `src-tauri/src/providers/copilot/mod.rs`

**Interfaces:**

- Detection and runtime candidate traversal use editor JSON (`apps.json`, `hosts.json`) and `hosts.yml` `oauth_token` only.
- macOS provider code never executes `gh auth token` and never queries generic credentials; no subprocess fallback is part of the macOS flow.

- [ ] Add tests using injected `TextFileAccess`, `GhTokenCommand`, and `CredentialAccess` fakes: editor token and hosts.yml token are detected; no file token yields unavailable; the gh fake and credential fake receive zero calls during detection and runtime refresh.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml providers::copilot::auth::tests` and verify the no-subprocess assertion fails against current traversal order.
- [ ] Change `visit_candidates` and `visit_detection_candidates` to share the approved file-source policy. Compile the no-gh/no-credential behavior for macOS; if non-macOS retains another policy, gate it explicitly and keep its behavior unchanged.
- [ ] Remove macOS `SystemCredentials` Security.framework methods/imports and remove `run_gh_token_command` from macOS reachable code. Retain safe direct file parsing for `GH_CONFIG_DIR` and `XDG_CONFIG_HOME` paths.
- [ ] Run Copilot tests and `cargo check --manifest-path src-tauri/Cargo.toml --target aarch64-apple-darwin`.
- [ ] Commit only this task's files with message `fix: keep Copilot auth file-backed on macOS`.

### Task 6: Make Antigravity quota local-RPC-only on macOS

**Files:**

- Modify: `src-tauri/src/providers/antigravity/mod.rs`, `src-tauri/src/providers/antigravity/auth.rs`, `src-tauri/src/providers/antigravity/client.rs`

**Interfaces:**

- `AntigravityProvider::refresh_inner` returns a snapshot only from parsed local language-server RPC; on local discovery/RPC/parse failure it returns `AntigravityError::Unavailable`.
- `AntigravityClient::call_language_server` uses a dedicated loopback client configured with `.no_proxy()`; cloud/token client paths are unreachable on macOS quota refresh.

- [ ] Add tests for each local RPC success response shape and for no server, failed RPC, and malformed quota results. Inject/fake the cloud client or count calls to prove the cloud quota and token-refresh endpoints are never invoked by the macOS refresh path.
- [ ] Add a client test with a deliberately configured HTTP proxy and a loopback mock RPC to prove local RPC reaches the mock directly; add an outbound mock-server test proving the separate remote client remains proxy-capable for non-quota code paths that remain in use.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml providers::antigravity::` and confirm unavailable test cases fail before removing fallback.
- [ ] Remove macOS Antigravity Keychain detection/auth candidates and refresh-token/cloud fallback. Keep local language-server discovery and RPC parsing.
- [ ] Set `.no_proxy()` only on the local client. Do not disable proxy handling on any public reqwest client.
- [ ] Run Antigravity module tests and `cargo check --manifest-path src-tauri/Cargo.toml --target aarch64-apple-darwin`.
- [ ] Commit only this task's files with message `fix: use local Antigravity quota RPC on macOS`.

### Task 7: Make Claude Code file/environment-only and remove account probes

**Files:**

- Modify: `src-tauri/src/providers/claude/auth.rs`, `src-tauri/src/providers/claude/accounts.rs`, `src-tauri/src/providers/claude/mod.rs`

**Interfaces:**

- `load_candidates` returns only `.credentials.json` file candidates and, for standard Claude Code scope, the `CLAUDE_CODE_OAUTH_TOKEN` inference-only candidate.
- Claude account discovery uses file-backed identities only. Claude Desktop config/Cookies are presence checks solely to report Desktop-only unsupported; contents are not read beyond the existing config token-cache presence check and cookies existence check.

- [ ] Add tests proving file credential detection/write-back works, environment token is inference-only and not refreshed or persisted, Keychain-only credentials are ignored, account discovery does not call credential-store probes, and Desktop-only data maps to the dedicated unsupported error.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml providers::claude::auth::tests` and `cargo test --manifest-path src-tauri/Cargo.toml providers::claude::accounts::tests` and verify Keychain-only cases no longer produce candidates.
- [ ] Remove `CredentialSource::Keychain`, Keychain candidate enumeration/read/write, `has_local_credentials` existence checks, and `accounts.rs` `generic_password_service_exists` probes. Keep atomic `.credentials.json` refresh write-back and the environment-token non-refresh behavior.
- [ ] Map Desktop-only detection and file/auth failures into existing `ProviderErrorKind` values without exposing local config contents or tokens.
- [ ] Run both Claude module test commands and the macOS target check.
- [ ] Commit only this task's files with message `fix: keep Claude Code auth file-backed on macOS`.

### Task 8: Normalize local credential errors and retain stale provider state

**Files:**

- Modify: `src-tauri/src/models.rs`, `src-tauri/src/providers/codex/mod.rs`, `src-tauri/src/providers/cursor/mod.rs`, `src-tauri/src/providers/copilot/mod.rs`, `src-tauri/src/providers/antigravity/mod.rs`, `src-tauri/src/providers/claude/mod.rs`
- Modify: `src-tauri/src/service.rs`, `src-tauri/src/settings.rs`, `src-tauri/src/commands/provider.rs`
- Modify: `src/lib/types.ts`, `src/lib/Dashboard.svelte`, `src/App.test.ts`

**Interfaces:**

- Add only the error distinctions required by the spec to `ProviderErrorKind`; preserve JSON camelCase serialization and the current `ProviderViewState` fields.
- Failed provider refresh continues through `merge_refresh_result`, retaining `snapshot`, setting `error`/`error_kind`, and deriving `stale` from snapshot age.

- [ ] Add service tests for one local-credential failure after a previously successful snapshot: assert the provider remains in settings, the exact snapshot is retained, the error category is set, and stale state is based on snapshot age. Add equivalent coverage for an instance with no previous snapshot.
- [ ] Add frontend tests that local-source errors display provider error text and the last snapshot's update time without hiding or deleting the provider instance.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml service::tests::failed_refresh_preserves_last_successful_snapshot_without_forcing_stale` and targeted frontend tests.
- [ ] Map missing local source, unreadable source, malformed credentials, expired token, refresh failure, vault-key failure, Antigravity local service unavailable, and Claude Desktop-only to existing or newly added `ProviderErrorKind` values. Do not parse secret-bearing text to infer categories.
- [ ] Confirm `settings.apply_credential_detection` treats a failed or absent local probe as no new detected credentials and never disables an already enabled provider; add regression coverage if needed.
- [ ] Run service/provider state tests and `pnpm test -- src/App.test.ts`.
- [ ] Commit only this task's files with message `fix: preserve provider instances on credential errors`.

### Task 9: Prove the macOS runtime has no Keychain path

**Files:**

- Modify: `scripts/verify/verify-platform-smoke.js` or add `scripts/verify/verify-macos-no-keychain.js`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` only if the platform/dependency check identifies a remaining package edge

- [ ] Add a repository verifier that scans production Rust sources and macOS Cargo dependency metadata for `security_framework`, `SecItem`, `generic_password`, `keychain_access`, `gh auth token`, and Keychain-specific provider variants. Exclude docs, test fixture strings, and Windows credential APIs; fail with the exact file and match.
- [ ] Run the verifier before the final fixes and confirm it catches a temporary test-only fixture match; remove that fixture and verify the production source check is meaningful.
- [ ] Run `cargo check --manifest-path src-tauri/Cargo.toml --target aarch64-apple-darwin`, Windows and Linux target checks, all five provider module tests, vault tests, detection tests, service tests, and `pnpm test` for frontend tests.
- [ ] Inspect Cargo tree for the macOS target and confirm `security-framework` is absent. Confirm Windows Credential Manager and Linux Secret Service dependencies remain.
- [ ] Run `git diff --check`, inspect `git status --short`, and ensure no token, auth JSON, database, vault, proxy credential, `.superpowers/`, or unrelated user file is staged.
- [ ] Commit verifier and any necessary final corrections with message `test: enforce zero macOS Keychain access`.

## Phase 1 Exit Gate

Do not begin the next phase until all Phase 1 target checks and tests pass, the repository verifier reports zero product-code Keychain paths, and a macOS development launch plus startup detection, provider enablement, periodic/manual refresh, token refresh, Codex reset claim, and Claude account discovery have been observed without a Keychain prompt.

## Later Phases (Not Executed by This Plan)

1. **UI window refactor:** after Phase 1 is accepted, use the already approved provider settings and independent popup design to create a separate implementation plan. Remove the total app icon on macOS, keep settings accessible with all providers disabled, and retain provider identity and error/cache timestamps in the popup.
2. **Proxy live acceptance:** after UI work is accepted, run the five-provider end-to-end acceptance from the spec with `HTTP_PROXY` and `HTTPS_PROXY` injected into the test process. Verify detection, explicit enablement, periodic refresh while popup is closed, manual refresh, supported token refresh/write-back, stale cache on failure, and Antigravity loopback bypass. Do not fall back to direct network access if the proxy is unavailable.

These later phases have independent review and acceptance gates; they are intentionally excluded from Phase 1 files and commits.
