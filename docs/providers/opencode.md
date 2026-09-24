# OpenCode Go

Quota01 tracks the **OpenCode Go** subscription quota for the OpenCode CLI. It no longer reads local
OpenCode databases or session logs — this card is now an account/subscription monitor, like the other
providers.

## What it tracks

| Metric  | Meaning                            |
| ------- | ---------------------------------- |
| Session | OpenCode Go rolling 5-hour window  |
| Weekly  | OpenCode Go weekly window          |
| Monthly | OpenCode Go monthly window         |

All three come from OpenCode's hosted usage endpoint (`https://opencode.ai/zen/go/v1/usage`), so they
include usage from every device signed in to the same Go subscription and reflect the limits OpenCode
enforces. That endpoint returns only these quota buckets — there is no per-day usage history — so
Quota01 shows no Today / Yesterday / Last 30 Days rows for this provider.

OpenCode's endpoint reports whole-number percentages, so any usage below 1% arrives as `0`. Quota01
therefore shows sub-1% readings as `<1%`/`>99%` and never labels a `0` meter as an unused window; the
reset countdown is always shown instead.

## Credentials

OpenCode Go uses an API key. Add it in Customize, set `OPENCODE_GO_API_KEY`, or configure
`~/.config/quota01/opencode.json`. The opencode CLI is **not** required: as long as you have a Go
subscription, pasting a key in Customize is enough, and the card enables itself once a key is saved.

As a fallback, Quota01 also reads the Go key that the `opencode` CLI writes into its own `auth.json`
when you sign in to Go:

- data directory: `$OPENCODE_DATA_DIR`, else `$XDG_DATA_HOME/opencode`, else `~/.local/share/opencode`
- key: the `opencode-go` entry's `key` field in `auth.json`

Customize reports which source is active:

| Shown as              | Meaning                                                                   |
| --------------------- | ------------------------------------------------------------------------- |
| From Your Environment | `OPENCODE_GO_API_KEY` is set                                              |
| From Config File      | `~/.config/quota01/opencode.json` supplied the key                        |
| From a CLI Sign-In    | The key came from opencode's own `auth.json`; nothing is saved in Quota01 |
| Saved securely        | A key was saved in Customize                                              |
| Custom Key            | A saved key is overriding the environment or config file                  |

Every source except "Saved securely" and "Custom Key" can be overridden: open the API key section,
tick **Override With a Custom Key**, and paste your own key. Saved keys and the CLI sign-in key can
both be replaced this way, and removing a saved key falls back to the CLI credential instead of
dropping the card to "no key".

A Go subscription is required: a valid key without an active Go plan reports "OpenCode Go
subscription required".

## Troubleshooting

- **Add an OpenCode Go API key** — add a key in Customize or set `OPENCODE_GO_API_KEY`; the opencode
  CLI's `auth.json` also works if you signed in there.
- **The OpenCode Go API key is invalid** — check the key on opencode.ai.
- **OpenCode Go subscription required** — the key is valid but has no active Go plan.
- **The OpenCode Go API key could not be read or updated** — the system credential store failed and
  no CLI sign-in or environment key is available as a fallback.
