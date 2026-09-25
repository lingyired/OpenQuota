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

Sign in from Quota01. Choose **Start Sign-In** on the WorkBuddy card, either in **Customize** or on
the dashboard when the card reports that sign-in is required. Quota01 opens the WorkBuddy
authorization page in your browser and also shows the link so it can be copied; confirm the sign-in
there. Quota01 then waits for the token, reads the account profile, and stores the session in its
own encrypted credential vault, protected by the operating system credential store. There is no API
key to paste.

Refreshes go to WorkBuddy's token endpoint and the renewed session is written back to Quota01's
vault by Quota01 itself. The session is never sent to the frontend or written to logs. Choosing
**Disconnect** removes it from the vault.

Older WorkBuddy and CodeBuddy releases stored a plaintext login file on your computer. Quota01
still reads that file when it exists and Quota01 has no session of its own, so an existing setup
keeps working without any action. WorkBuddy 5.6 and later write the tokens into that file as an
encrypted envelope instead, which Quota01 cannot read; for those releases, sign in from Quota01.

The sign-in panel reports only the session Quota01 itself holds. While the local login file is the
source that is supplying data, the cards keep showing usage, but the panel reads as not connected
and offers **Start Sign-In**; signing in from Quota01 moves the account onto Quota01's own session.

## Troubleshooting

- **Not logged in** — choose **Start Sign-In** on the WorkBuddy card and confirm the sign-in in
  your browser.
- **Login data is invalid** — choose **Start Sign-In** on the WorkBuddy card and sign in again.
- **Login credentials are encrypted** — WorkBuddy 5.6 encrypts the login data it keeps on this
  computer, so it cannot be read directly. Choose **Start Sign-In** on the WorkBuddy card and sign
  in from Quota01 to connect.
- **Token expired** — the saved refresh token could not renew the session; sign in from Quota01
  again.
- **Blocked by the upstream WAF** — WorkBuddy rate-limited the request; try again later.
- **Usage data is partial** — the credit balance loaded but some usage records could not. The
  balance is still accurate.
