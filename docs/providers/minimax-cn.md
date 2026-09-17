# MiniMax CN

Quota01 tracks the Session and Weekly quotas of a MiniMax Token Plan bought in mainland China.

MiniMax runs two separate account systems. Quota01 ships both as separate providers because the API
keys are not interchangeable: **MiniMax** uses `www.minimax.io` and the global console, while
**MiniMax CN** uses `www.minimaxi.com` and the mainland platform. Add whichever site you subscribed
to.

## What it tracks

| Metric       | Meaning                                      |
| ------------ | -------------------------------------------- |
| Session (5h) | Usage remaining in the rolling 5-hour window |
| Weekly       | Usage remaining in the rolling 7-day window  |

Both metrics are pinned by default.

## Setup

Create or view the Token Plan key in the
[MiniMax China console](https://platform.minimaxi.com/subscribe/token-plan), then add it in
**Customize** in Quota01. Saved keys are stored in the encrypted credential vault protected by the
operating system credential store. Quota01 also checks `MINIMAX_CN_API_KEY` and
`~/.config/quota01/minimax-cn.json`; a key saved in the app takes priority.

This provider uses MiniMax's mainland endpoint, `https://www.minimaxi.com/v1/token_plan/remains`.

## Troubleshooting

- **Add an API key** — add a MiniMax CN Token Plan key in Customize or provide it through a supported
  external source.
- **API key invalid** — create or verify the key in the
  [MiniMax China console](https://platform.minimaxi.com/).
- **No active token plan** — subscribe to a Token Plan in the
  [MiniMax China console](https://platform.minimaxi.com/subscribe/token-plan).
- **Usage unavailable** — check the connection and refresh again.
