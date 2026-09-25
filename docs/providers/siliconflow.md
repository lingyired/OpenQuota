# SiliconFlow

Quota01 tracks the account balances of a SiliconFlow account registered on the international
console.

SiliconFlow runs two consoles with separate accounts and separate API keys. Quota01 ships both as
separate providers: **SiliconFlow** uses `api.siliconflow.com` and quotes balances in US dollars,
while **SiliconFlow CN** uses `api.siliconflow.cn` and quotes balances in yuan. Add whichever
console issued your key.

## What it tracks

| Metric            | Meaning                                            |
| ----------------- | -------------------------------------------------- |
| Balance           | Total balance available to the account             |
| Granted Balance   | Promotional credit granted by SiliconFlow          |
| Recharged Balance | Credit you paid for                                |

Balance and Granted Balance are pinned by default. The endpoint reports balances only — it exposes
no monthly cap — so the card shows values instead of a usage meter.

## Setup

Create or view the key in the [SiliconFlow console](https://cloud.siliconflow.com/account/ak), then
add it in **Customize** in Quota01. Saved keys are stored in the encrypted credential vault
protected by the operating system credential store. Quota01 also checks `SILICONFLOW_API_KEY` and
`~/.config/quota01/siliconflow.json`; a key saved in the app takes priority.

This provider uses `https://api.siliconflow.com/v1/user/info`.

## Troubleshooting

- **Add an API key** — add a SiliconFlow key in Customize or provide it through a supported external
  source.
- **API key invalid** — create or verify the key in the
  [SiliconFlow console](https://cloud.siliconflow.com/account/ak).
- **Usage unavailable** — check the connection and refresh again. SiliconFlow reports rejected
  requests with a business code inside an HTTP 200 response; Quota01 validates that code, so an
  unusable key is reported as an invalid key rather than as an outage.
