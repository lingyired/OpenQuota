# Claude Code

Quota01 tracks Claude subscription limits and local Claude usage history.

## What it tracks

| Metric                           | Meaning                                                      |
| -------------------------------- | ------------------------------------------------------------ |
| Session (5h)                     | Usage remaining in the current session window                |
| Weekly                           | Usage remaining in the weekly window                         |
| Sonnet / Fable                   | Model-specific limits when they are reported for the account |
| Extra Usage                      | Extra-usage allowance or spending reported by Claude         |
| Today / Yesterday / Last 30 Days | Tokens and estimated spend calculated from local usage logs  |
| Usage Trend                      | Recent local usage over time                                 |

## Sign-in and local data

Sign in with Claude Code by running `claude`. Quota01 reuses the credentials maintained by the
CLI, including `CLAUDE_CONFIG_DIR` when it is set. Refreshed CLI credentials are saved back to the
same source when possible.

## Keychain access

Quota01 reads the `Claude Code*-credentials` Keychain entries the CLI maintains and writes refreshed
credentials back to them. macOS asks for authorization the first time an entry is read, so those
entries stay untouched until you turn the Claude card on by hand in **Customize → Providers**.
Enabling one Claude card grants every Claude account card. File-based logins
(`CLAUDE_CONFIG_DIR/.credentials.json`) keep working automatically, because reading them never
prompts; a Keychain-only login reports the provider as unknown instead of "not logged in".

## Multiple accounts

Quota01 discovers separate Claude Code logins that use custom `CLAUDE_CONFIG_DIR` homes and shows
each account as its own card with independent limits, plan, and local usage history. Logins belonging
to the same Claude account are combined automatically.

Account cards can be renamed from Customize or from the dashboard. If a login is removed, its card
is hidden and returns with its previous customization when the login is detected again.

Live subscription limits currently require a Claude Code login. On macOS, Quota01 can recognize
that Claude Desktop is installed, but it does not reuse Desktop's encrypted session. Run `claude`
and sign in once if Desktop is your only Claude login.

Spend history is calculated locally from Claude usage logs. It can also include compatible Claude
usage recorded by pi and, on macOS, Claude's local agent-mode sessions. These local records are not
uploaded by Quota01.

## Troubleshooting

- **Not logged in** — run `claude`, complete sign-in, then refresh Quota01.
- **Claude Desktop login found** — sign in once through the Claude Code CLI.
- **Session or token expired** — sign in again with `claude`.
- **No local history** — use Claude Code normally and check whether `CLAUDE_CONFIG_DIR` points to
  the directory containing your Claude data.
