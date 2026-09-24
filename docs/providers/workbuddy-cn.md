# Workbuddy CN

Quota01 tracks WorkBuddy credits and usage. WorkBuddy is Tencent's mainland-China AI coding
assistant, so this provider is listed as the `-cn` variant.

## What it tracks

| Metric           | Meaning                                                                    |
| ---------------- | -------------------------------------------------------------------------- |
| 近期到期的积分包 | Credits in the package that expires soonest                                |
| Credits          | Available (remaining) credits, summed across every valid credit package    |
| 可用积分包       | Each valid credit package with its own remaining/total progress and expiry |
| Today            | Tokens used today                                                          |
| Yesterday        | Tokens used yesterday                                                      |
| Last 30 Days     | Tokens used over the last 30 days                                          |
| Usage Trend      | Daily token trend with per-model breakdown                                 |

The first two metrics are pinned by default.

**Credits always means remaining credits.** Used-versus-total is only shown inside each credit
package row, so the number in the menu bar and the number in the panel can never disagree.

## Setup

Sign in to the WorkBuddy or CodeBuddy desktop app. Quota01 reuses the login data those apps already
stored on your computer, so there is no API key to paste and nothing to configure in **Customize**.
The provider appears once that login data exists.

### WorkBuddy 5.6 and later

WorkBuddy 5.6 started storing `accessToken` / `refreshToken` as an encrypted envelope
(`{"$wbEncrypted": 1, "envelope": "..."}`), so the login file can no longer be read directly. When
Quota01 meets that envelope it looks for a readable copy of the same account (matched by `uid`) in
[workbuddy-switch](https://github.com/changexbc/workbuddy-switch)'s account store,
`~/.wb-switch/accounts.json`, and uses it **read-only**:

- Quota01 never writes back to that store and never spends its refresh token. Consuming a rotating
  refresh token that cannot be saved would break the other app's session too.
- If the borrowed copy stops working, open workbuddy-switch so it refreshes its own token.
- If no readable copy exists, Quota01 reports that the credentials are encrypted instead of
  claiming you are signed out.

## Troubleshooting

- **Not logged in** — sign in to WorkBuddy or CodeBuddy, then refresh.
- **Login data is invalid** — sign in to WorkBuddy or CodeBuddy again.
- **Login credentials are encrypted** — WorkBuddy 5.6 encrypted its local login data and no
  readable copy was found. Sign in again in WorkBuddy or CodeBuddy, or launch workbuddy-switch so a
  readable copy becomes available.
- **Token expired** — the saved refresh token could not renew the session; sign in again.
- **Blocked by the upstream WAF** — WorkBuddy rate-limited the request; try again later.
- **Usage data is partial** — the credit balance loaded but some usage records could not. The
  balance is still accurate.
