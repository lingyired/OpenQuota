# Trae CN

Usage01 tracks the available credit balance for a Trae CN account.

## What it tracks

| Metric  | Meaning                                                   |
| ------- | --------------------------------------------------------- |
| Credits | Remaining Trae credits across the active entitlement plan |
| Status  | Plan state shown when the account has no active credits   |

The credit meter uses the usage summary returned by Trae's CN billing endpoint:
`remaining = total_amount - consumed_amount`.

## Setup

When Usage01 reports that Trae CN needs sign-in, use **Open Sign-In** directly on the Trae CN card.
The same controls remain available in **Customize**. Complete the login in the window that opens,
return to Usage01, and choose **I Have Signed In**.

Usage01 reads the `X-Cloudide-Session` cookie from its own WebView and stores it in the
operating system credential store. The session is never sent to the frontend, written to logs, or
shared with a browser profile. Usage01 exchanges it for a short-lived JWT in Rust before
requesting credits. Choosing **Disconnect** removes both the stored session and the matching cookie
from the app's WebView data store.

This integration targets the mainland China service at `www.trae.cn` and `api.trae.cn`. Trae
international credentials are separate and are not accepted by this provider.

## Troubleshooting

- **Sign in first** — complete the WebView login before choosing **I Have Signed In**.
- **Session expired** — choose **Open Sign-In** again and recapture the session.
- **No active credits** — the account currently has no active credit plan.
- **Usage unavailable** — check the connection and refresh again. Trae's private endpoint may be
  rate limited or temporarily unavailable.
