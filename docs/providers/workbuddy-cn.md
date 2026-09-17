# Workbuddy CN

Quota01 tracks WorkBuddy credits and usage. WorkBuddy is Tencent's mainland-China AI coding
assistant, so this provider is listed as the `-cn` variant.

## What it tracks

| Metric           | Meaning                                                                       |
| ---------------- | ----------------------------------------------------------------------------- |
| 近期到期的积分包 | Credits in the package that expires soonest                                   |
| Credits          | Available (remaining) credits, summed across every valid credit package       |
| 可用积分包       | Each valid credit package with its own remaining/total progress and expiry     |
| Today            | Tokens used today                                                             |
| Yesterday        | Tokens used yesterday                                                         |
| Last 30 Days     | Tokens used over the last 30 days                                             |
| Usage Trend      | Daily token trend with per-model breakdown                                    |

The first two metrics are pinned by default.

**Credits always means remaining credits.** Used-versus-total is only shown inside each credit
package row, so the number in the menu bar and the number in the panel can never disagree.

## Setup

Sign in to the WorkBuddy or CodeBuddy desktop app. Quota01 reuses the login data those apps already
stored on your computer, so there is no API key to paste and nothing to configure in **Customize**.
The provider appears once that login data exists.

## Troubleshooting

- **Not logged in** — sign in to WorkBuddy or CodeBuddy, then refresh.
- **Login data is invalid** — sign in to WorkBuddy or CodeBuddy again.
- **Token expired** — the saved refresh token could not renew the session; sign in again.
- **Blocked by the upstream WAF** — WorkBuddy rate-limited the request; try again later.
- **Usage data is partial** — the credit balance loaded but some usage records could not. The
  balance is still accurate.
