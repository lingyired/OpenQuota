# Antigravity

Quota01 tracks the shared Gemini and Claude quota pools reported by Antigravity.

## What it tracks

| Metric        | Meaning                                             |
| ------------- | --------------------------------------------------- |
| Session (5h)  | Shared Gemini pool in the rolling 5-hour window     |
| Weekly        | Shared Gemini pool in the weekly window             |
| Claude        | Shared non-Gemini pool in the rolling 5-hour window |
| Claude Weekly | Shared non-Gemini pool in the weekly window         |

Only the pools available to the signed-in account are shown.

## Sign-in and local data

Sign in through Antigravity or run `agy`. Quota01 reuses Antigravity's locally stored credentials
and can also discover a compatible running Antigravity language server. No API key needs to be added
to Quota01.

## Keychain access

Antigravity's credentials live only in the `gemini` / `antigravity` Keychain entry. macOS asks for
authorization the first time that entry is read, so Quota01 leaves it alone until you turn the
Antigravity card on by hand in **Customize → Providers**. Until then the card has nothing to show:
there is no file-based fallback, and detection reports the provider as unknown rather than
"not signed in", because the entry was never read.

## Troubleshooting

- **Not signed in** — open Antigravity or run `agy`, complete sign-in, then refresh Quota01.
- **Sign-in expired** — authenticate again in Antigravity or with `agy`.
- **Credentials could not be read** — reopen Antigravity so its local sign-in state can be restored.
- **Usage temporarily unavailable** — wait briefly and refresh again.
