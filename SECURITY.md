# Security Policy

## Supported versions

Security fixes are provided for the latest published version of Usage01. Before reporting a
problem, check whether it is still present in the
[latest release](https://github.com/deviffyy/OpenQuota/releases/latest).

## Credential storage

Usage01 stores provider API keys and WebView session cookies in a single encrypted credential vault.
The vault uses ChaCha20-Poly1305 with a random nonce and an authenticated payload. Its random
encryption key is stored in the operating system credential store, so the system protects one
Usage01-owned vault key instead of prompting separately for every provider credential.

## Reporting a vulnerability

Please do not open a public issue for security vulnerabilities.

Use GitHub's
[private vulnerability reporting](https://github.com/deviffyy/OpenQuota/security/advisories/new)
to report the issue confidentially.

Include:

- The affected Usage01 version and operating system
- A clear description of the vulnerability and its impact
- Reproduction steps or a proof of concept when available
- Any suggested mitigation

Do not include real credentials, access tokens, or private account data. Reports will be reviewed
privately, and a fix or mitigation will be coordinated before public disclosure when the issue is
confirmed.
