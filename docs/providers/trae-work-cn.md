# TraeWork CN

Usage01 tracks the available credit balance for a TraeWork CN account.

## What it tracks

| Metric           | Meaning                                                                       |
| ---------------- | ----------------------------------------------------------------------------- |
| Credits          | Total remaining credits from the usage summary                                |
| Work 专属积分    | Remaining credits available only through TraeWork                             |
| 不含 Work 总积分 | Total remaining credits excluding Work-exclusive packages                     |
| 近期到期的积分包 | Remaining balance of the nearest unexpired package with value                 |
| 可用积分包       | Every unexpired package with remaining value, including unlimited free access |

The total meter uses the usage summary returned by Trae's CN billing endpoint:
`remaining = total_amount - consumed_amount`. Individual package balances use
`credits_limit - credits_amount`, while Work-exclusive packages use
`entitlement_base_info.available_endpoint == 1`.

## Setup

When Usage01 reports that TraeWork CN needs sign-in, use **Open Sign-In** directly on the TraeWork CN card.
The same controls remain available in **Customize**. Complete the login in the window that opens;
Usage01 captures the session automatically when that window closes. **I Have Signed In** remains
available as a manual fallback.

Usage01 reads the `X-Cloudide-Session` cookie from its own WebView and stores it in the encrypted
credential vault protected by the operating system credential store. The session is never sent to
the frontend, written to logs, or shared with a browser profile. Usage01 exchanges it for a short-lived JWT in Rust before
requesting credits. Choosing **Disconnect** removes both the stored session and the matching cookie
from the app's WebView data store.

This provider targets the mainland China service at `www.trae.cn` and `api.trae.cn`, and is enabled
when either `cn.trae.solo.app` or `cn.trae.app` is installed. Trae international credentials are
separate and are not accepted by this provider.

## Troubleshooting

- **Sign in first** — complete the WebView login before choosing **I Have Signed In**.
- **Session expired** — choose **Open Sign-In** again and recapture the session.
- **No active credits** — the account currently has no active credit plan.
- **Usage unavailable** — check the connection and refresh again. Trae's private endpoint may be
  rate limited or temporarily unavailable.
