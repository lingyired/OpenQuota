# Z.ai CN

Quota01 tracks quota information for the 智谱 BigModel GLM Coding Plan in mainland China.

Z.ai and BigModel run the same GLM Coding Plan backend under two brands. Quota01 ships both as
separate providers because the accounts and API keys are not interchangeable: **Z.ai** signs in at
`api.z.ai`, while **Z.ai CN** signs in at `open.bigmodel.cn`. Add whichever site you subscribed to.

## What it tracks

| Metric       | Meaning                                      |
| ------------ | -------------------------------------------- |
| Session (5h) | Usage remaining in the rolling 5-hour window |
| Weekly       | Usage remaining in the rolling 7-day window  |
| Web Searches | Monthly web-search allowance remaining       |

The first two metrics are pinned by default.

## Setup

Create an API key in the [BigModel API key page](https://open.bigmodel.cn/user-center/apikeys), then
add it in **Customize** in Quota01. Saved keys are kept in the encrypted credential vault protected
by the operating system credential store. Quota01 also checks `ZAI_CN_API_KEY`, `BIGMODEL_API_KEY`,
`ZHIPU_API_KEY`, and `~/.config/quota01/zai-cn.json`; a key saved in the app takes priority.

The key must belong to an account with an active GLM Coding Plan.

## Troubleshooting

- **Add an API key** — add a BigModel key in Customize or provide one through a supported external
  source.
- **API key invalid** — verify the key at the
  [BigModel API key page](https://open.bigmodel.cn/user-center/apikeys).
- **No active coding plan** — confirm that the account has an active GLM Coding Plan, and subscribe
  at [bigmodel.cn/glm-coding](https://bigmodel.cn/glm-coding) if it does not.
- **Usage unavailable** — check the connection and refresh again.
