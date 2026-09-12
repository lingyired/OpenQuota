# Z.ai

Usage01 tracks quota information for the Z.ai GLM Coding Plan.

## What it tracks

| Metric       | Meaning                                      |
| ------------ | -------------------------------------------- |
| Session (5h) | Usage remaining in the rolling 5-hour window |
| Weekly       | Usage remaining in the rolling 7-day window  |
| Web Searches | Monthly web-search allowance remaining       |

## Setup

Add a Z.ai API key from **Customize** in Usage01. Saved keys are kept in the encrypted credential
vault protected by the operating system credential store. Usage01 also checks `ZAI_API_KEY`, `GLM_API_KEY`,
`~/.config/usage01/zai.json`, and `~/.config/zai/key.json`; a key saved in the app takes priority.

The key must belong to an account with an active GLM Coding Plan.

## Troubleshooting

- **Add an API key** — add a key in Customize or provide one through a supported external source.
- **API key invalid** — verify the key at [Z.ai API Keys](https://z.ai/manage-apikey/apikey-list).
- **No active coding plan** — confirm that the account has an active GLM Coding Plan.
- **Usage unavailable** — check the connection and refresh again.
