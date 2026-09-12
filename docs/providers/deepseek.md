# DeepSeek

Usage01 tracks the current DeepSeek API balance through the official balance endpoint and a
DeepSeek API key.

## What it tracks

| Metric  | Meaning                                                         |
| ------- | --------------------------------------------------------------- |
| Balance | Current total balance returned for each currency on the account |
| Status  | Whether DeepSeek reports the API account as available           |

DeepSeek can return balances in more than one currency. Usage01 keeps each currency separate
instead of combining or converting them.

## Setup

Create an API key in the [DeepSeek Platform](https://platform.deepseek.com/api_keys), then add it in
**Customize** in Usage01. Saved keys use the encrypted credential vault protected by the operating
system credential store. Usage01 also
checks `DEEPSEEK_API_KEY` and `~/.config/usage01/deepseek.json`; a key saved in the app takes
priority.

This provider uses the official `https://api.deepseek.com/user/balance` API. DeepSeek API keys do
not provide access to the private usage-history endpoints used by the platform dashboard, so
Usage01 does not request or store the separate platform login token.

## Troubleshooting

- **Add an API key** — add a DeepSeek API key in Customize or provide it through a supported external source.
- **API key invalid** — create or verify the key at [DeepSeek API Keys](https://platform.deepseek.com/api_keys).
- **Balance unavailable** — confirm that the API account is active and try again.
