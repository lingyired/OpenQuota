# Codex

Quota01 tracks Codex subscription limits and usage recorded by the Codex CLI.

## What it tracks

| Metric                           | Meaning                                                      |
| -------------------------------- | ------------------------------------------------------------ |
| Session (5h)                     | Usage remaining in the current session window                |
| Weekly                           | Usage remaining in the weekly window                         |
| Spark / Spark Weekly             | Model-specific limits when they are reported for the account |
| Extra Usage                      | Additional usage credits reported by Codex                   |
| Rate Limit Resets                | Available reset credits                                      |
| Today / Yesterday / Last 30 Days | Tokens, model usage, and estimated spend from local logs     |
| Usage Trend                      | Recent local usage over time                                 |

## Sign-in and local data

Sign in with the Codex CLI by running `codex` and choosing your ChatGPT account. Quota01 reads the
same authentication data and respects `CODEX_HOME` when it is set. API-key-only sessions can produce
local usage history, but they cannot provide ChatGPT subscription limits.

## Keychain access

Besides `CODEX_HOME/auth.json`, Quota01 reads the `Codex Auth` Keychain entry and writes refreshed
credentials back to it. macOS asks for authorization the first time that entry is read, so Quota01
leaves it alone until you turn the Codex card on by hand in **Customize → Providers**. File-based
logins keep working automatically, because reading them never prompts.

Spend history is calculated locally from the Codex `sessions` and `archived_sessions` logs. Compatible
Codex usage recorded by pi can also be included. Quota01 does not upload these local records.

## Troubleshooting

- **Not logged in** — run `codex`, sign in with ChatGPT, then refresh Quota01.
- **Subscription usage unavailable** — replace an API-key-only login with a ChatGPT login.
- **Session expired or revoked** — sign in again with `codex`.
- **No local history** — check the active Codex data directory and the value of `CODEX_HOME`.
