# DeepSeek

Quota01 tracks DeepSeek account balance, cumulative spend, and today's spend by using the platform
session created by a secure WebView sign-in.

## What it tracks

| Metric       | Meaning                                                         |
| ------------ | --------------------------------------------------------------- |
| Balance      | Current total balance for each currency on the account          |
| Today Spend  | Spend recorded for the current local day                        |
| Total Spend  | Cumulative spend reported by the platform overview              |

DeepSeek can report amounts in more than one currency. Quota01 keeps each currency separate instead
of combining or converting them. Balance and Today Spend are pinned to the menu bar by default;
Total Spend can be pinned from **Customize**.

## Setup

1. Open **Customize** in Quota01 and select **DeepSeek**.
2. Click **Open Sign-In**.
3. Sign in at [platform.deepseek.com](https://platform.deepseek.com) in the window that opens.
4. Close the sign-in window. Quota01 reads the DeepSeek browser session and connects automatically.

If automatic capture does not complete, click **I Have Signed In** in Customize. Quota01 reads the
DeepSeek `userToken` from the sign-in window's local storage and stores it in its encrypted
credential vault protected by the operating system credential store. It also checks
`DEEPSEEK_USER_TOKEN` and `~/.config/quota01/deepseek.json`; a session saved in the app takes
priority.

No DeepSeek API key is required. The browser session token is separate from an API key and can
access platform account data, so a separate DeepSeek account is recommended for balance monitoring.

## Data source and limitations

Quota01 calls the private platform endpoints
`/api/v0/users/get_user_summary` and `/api/v0/usage/by_api_key/cost` with
`Authorization: Bearer <userToken>`. These endpoints are not part of the public DeepSeek API and
may change without notice. Usage data can also be delayed by several minutes.

The `userToken` is a browser session credential and can expire. If that happens, sign in again and
open the DeepSeek sign-in window again from **Customize**.

## Troubleshooting

- **Sign-in required** — open the DeepSeek sign-in window in Customize and sign in again. You can
  also set `DEEPSEEK_USER_TOKEN` for headless setups.
- **Session expired** — open the DeepSeek sign-in window and sign in again.
- **Usage unavailable** — check the internet connection and confirm the private platform endpoints
  still respond in the browser.
