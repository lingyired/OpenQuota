# WorkBuddy Self-Contained Sign-In Design

## Goal

Give Quota01 a WorkBuddy credential it obtains and owns itself, so credit and usage tracking no
longer depends on reading a file WorkBuddy has encrypted, and no longer depends on any third-party
application running on the same machine.

This replaces the interim `~/.wb-switch/accounts.json` borrow, which was rejected: depending on
another app's account store means WorkBuddy breaks whenever that app is absent, logged out, or
stale.

## Background: why the existing read path broke

Quota01's WorkBuddy provider originally read `accessToken`/`refreshToken` as plaintext strings from
the login file WorkBuddy and CodeBuddy maintain:

```text
~/Library/Application Support/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info
```

WorkBuddy 5.6.2, auto-installed on 2026-09-22 at 16:02, changed that file. `accessToken`,
`refreshToken`, `nickname`, and `phoneNumber` are now encrypted envelopes:

```json
{ "$wbEncrypted": 1, "envelope": "<base64>" }
```

The envelope decodes to `{"suite":1,"keyId":"…","nonce":<12B>,"authTag":<16B>,"ciphertext":…}` and
is AES-256-GCM with additional authenticated data bound to `(framing, scheme, suite, keyId,
sequence, final)`.

The key chain, established by reading the shipped WorkBuddy code:

- `keyId = sha256(key).hexdigest()[:16]`.
- The envelope's `keyId` (`9127dea1b44020a7` on this machine) identifies the **protector** key.
- `~/.workbuddy/keyblob` holds a data key wrapped by that same protector key.
- The protector derives from `atRestSecretKey`, supplied by `electron.workbuddyStorage.loggerGet()`
  — a native binding, meaning the secret is compiled into WorkBuddy's native build rather than
  stored in any user-readable file.

The connector master key (`~/.workbuddy/connectors/<uid>/.master.key`, mirrored under
`~/.workbuddy-key-fallback/connector-keys/<sha256(uid)[:32]>.key`) is **not** this key: its derived
key id is `ab0b10bc1ce66c6d`.

**Consequence.** Restoring the plaintext read would mean extracting a build-embedded secret from
WorkBuddy's native code. That is rejected: it breaks on every WorkBuddy release, and it would embed
a bypass of another vendor's at-rest credential encryption in Quota01.

This is corroborated independently by `workbuddy-switch` 0.1.47 (issues #74, #75, #94, #95). Its
authors preserve envelopes verbatim, short-circuit credential-dependent requests with a readable
error instead of sending an empty bearer token, and state in code that an envelope token "cannot be
decrypted into plaintext"; the plaintext credential must come from OAuth sign-in. No commit in that
project attempts decryption, and none of its code contains envelope decryption.

## Approach

Provider-local device-code authorization, completed in the user's browser.

1. `POST /v2/plugin/auth/state?platform=workbuddy` returns `{state, authUrl}`. Verified live on this
   machine: HTTP 200, `code: 0`, 204 ms, no credentials required.
2. Quota01 shows the `authUrl` and opens it in the user's default browser, where an existing
   CodeBuddy/WorkBuddy web session makes authorization a single confirmation.
3. Quota01 polls `GET /v2/plugin/auth/token?state=<state>` until the signed-in response carries an
   access token.
4. Quota01 reads `GET /v2/plugin/login/account?state=<state>` with the bearer token to learn
   `uid`, `nickname`, `email`, and enterprise fields.
5. The resulting session is stored in Quota01's own encrypted vault.
6. Later refreshes use the existing `POST /v2/plugin/auth/token/refresh` and write back to
   Quota01's own store.

The device-code flow is intentionally provider-local. Quota01's existing `WebviewAuth` abstraction
captures a cookie or `localStorage` value when a sign-in window closes, which cannot express a
server-side state exchange. Generalizing that abstraction for a single consumer would push
WorkBuddy-specific endpoints and semantics into a shared interface, so the flow lives in the
WorkBuddy provider until a second provider genuinely needs it.

## Credential precedence

1. **Quota01's own stored session** — primary. Quota01 owns it, so refresh results are written back.
2. **The WorkBuddy plaintext login file** — compatibility for WorkBuddy releases before 5.6.
3. **Neither available** — an actionable error, never a misleading "not logged in".

An encrypted envelope is never treated as a credential. Encountering one without any readable
source reports `CredentialsEncrypted` with text that names the cause and the fix.

## Components

### `providers/workbuddy/login.rs`

The device-code state machine: request a state, poll for a token, fetch the account profile. HTTP
access is injected so every branch is unit-testable without network access. Started logins are
keyed by a generated login id with an expiry, so a stale poll cannot be confused with a new login.

### `providers/workbuddy/session.rs`

Persists the session as a single JSON document through the existing `ApiKeyStore` /
`credential_vault` path, which is already the mechanism used for other providers' secrets and is
exposed through the provider trait's existing `session_status`, `save_session`, and `delete_session`
hooks.

Stored fields: `access_token`, `refresh_token`, `token_type`, `domain`, `uid`, `nickname`, `email`,
`enterprise_id`, `expires_at`, `refresh_expires_at`. No token is ever logged.

### Provider wiring

- Credential loading prefers the stored session, then the plaintext file, then fails with a typed
  error.
- Refresh writes back to whichever source supplied the credential; a Quota01-owned session is
  written to the vault, and the plaintext-file path keeps today's atomic write-back behavior.
- The encrypted-envelope detection, the `CredentialsEncrypted` error, and the
  `has_local_credentials` revision described below are retained.

### Commands

Three commands, dispatched by `provider_id` so the shape is not WorkBuddy-specific:
`start_provider_login`, `poll_provider_login`, and `cancel_provider_login`. They are registered in
the Tauri handler list and in `scripts/verify/verify-command-contract.js`.

### Frontend

A sign-in affordance on the WorkBuddy card, following `ProviderSessionActions.svelte`: start
sign-in, show the authorization link with an "open in browser" action (via the existing
`tauri_plugin_opener` path), poll until completion, and report cancellation or timeout. Copy is
localized through the existing backend-glossary mechanism, with real translations for `zh-CN` and
`zh-TW`.

## Error states

| Condition                                      | Kind                | Message intent                                                 |
| ---------------------------------------------- | ------------------- | -------------------------------------------------------------- |
| No readable credential at all                  | `credentialStorage` | Local login data is encrypted; sign in to grant Quota01 access |
| Sign-in cancelled or timed out                 | `authentication`    | The sign-in attempt expired; start again                       |
| `auth/state` or `auth/token` transport failure | `network`           | Could not reach WorkBuddy                                      |
| Stored session rejected                        | `authentication`    | Session expired; sign in again                                 |
| Vault read/write failed                        | `credentialStorage` | Credentials could not be read or updated                       |

## Revisions to earlier specs

`2026-09-11-workbuddy-provider-design.md` states that a login file which exists but lacks an access
token is not a detected credential. That is now deliberately revised:
`has_local_credentials` returns true when the login file exists, because treating an unreadable file
as "absent" makes credential detection classify the provider as `Absent`, which auto-disables and
hides the card — leaving the user with no way to see the real reason.

## Removal of the interim borrow

The following are removed: the `~/.wb-switch/accounts.json` path helper, the shared-store loader,
the `SharedStore` credential source, the read-only guard, and the tests covering them. The
envelope detection, the `CredentialsEncrypted` error and its mappings, the `has_local_credentials`
revision, and the localization entries stay.

## Tests

Rust:

- Device-code start/poll/login state machine: success, pending, expired, cancelled, transport
  failure, and a response whose `domain` does not match the requested host.
- Session round-trip through the vault, including refresh write-back.
- Credential precedence: stored session over the plaintext file; plaintext file over failure;
  envelope without a source reports `CredentialsEncrypted`.
- Retained envelope and detection behavior from the interim change.

Frontend:

- Sign-in affordance states: idle, awaiting authorization, error, success.
- Localization coverage for the new keys across locales, and the backend-glossary mappings.

Verification runs the repository's full gate: `cargo fmt --check`, `cargo clippy -D warnings`,
`cargo test --all-targets`, contract scripts, ESLint, Prettier, `svelte-check`, frontend tests, and
the production build.

A live acceptance step remains: the polling half of the device-code flow can only be proven by
completing one real authorization, which requires the user to confirm in their browser.

## Non-goals

- Decrypting `$wbEncrypted`, extracting WorkBuddy's build-embedded at-rest key, or reading
  `~/.workbuddy/keyblob`.
- Depending on `workbuddy-switch`, the CodeBuddy IDE, or any other application's account store.
- Reading the CodeBuddy IDE's Chromium Safe Storage credential. It is a viable no-scan source but
  it trades one application dependency for another; it can be added later behind the same
  precedence list if a no-scan path is wanted.
- Multiple WorkBuddy accounts. One account only; the storage shape keeps a `uid` so adding account
  cards later does not require a migration.
- The WorkBuddy AI (international) variant, which Quota01 does not expose today.
- Writing the Quota01-obtained plaintext token back into WorkBuddy's own login file.
