# Infini

Quota01 tracks the request quotas of an Infini (无问芯穹) Coding Plan.

A Coding Plan is a subscription to the `cloud.infini-ai.com/maas/coding` endpoint rather than a
balance on the GenStudio LLM API. The two products do not share keys: Coding Plan keys are issued as
`sk-cp-…`, and a GenStudio key (`sk-…`) is rejected by the usage endpoint.

## What it tracks

| Metric      | Meaning                                             |
| ----------- | --------------------------------------------------- |
| Session (5h) | Requests used in the rolling 5-hour bucket         |
| Weekly      | Requests used in the 7-day bucket                    |
| Monthly     | Requests used in the 30-day bucket                  |

Quotas count requests, not tokens. Session (5h) and Weekly are pinned by default.

The 5-hour bucket is a rolling window, so usage leaves it five hours after each request; the 7-day
and 30-day buckets reset on fixed boundaries (Mondays and the start of the billing cycle). The usage
endpoint returns counts only, without reset timestamps, so Quota01 shows the window length and the
counts rather than a reset countdown.

## Setup

Create or view the key in the [Infini console](https://cloud.infini-ai.com/platform/ai), then add it
in **Customize** in Quota01. Saved keys are stored in the encrypted credential vault protected by
the operating system credential store. Quota01 also checks `INFINI_API_KEY` and
`~/.config/quota01/infini.json`; a key saved in the app takes priority.

This provider uses `https://cloud.infini-ai.com/maas/coding/usage`. A plan can hold up to five
independent keys, so each key reports the quota of the whole subscription.

## Troubleshooting

- **Add an API key** — add a Coding Plan key in Customize or provide it through a supported external
  source.
- **API key invalid** — create or verify the key in the
  [Infini console](https://cloud.infini-ai.com/platform/ai). The endpoint reports a rejected key as
  `{"code":10021}`, sometimes alongside HTTP 401; Quota01 treats both the same way.
- **Usage unavailable** — check the connection and refresh again. A key without an active Coding
  Plan returns no quota buckets, which is reported as unavailable usage rather than as an invalid
  key.
