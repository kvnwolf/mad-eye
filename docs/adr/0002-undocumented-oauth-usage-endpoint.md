# 0002. Usage data comes from Claude Code's undocumented OAuth endpoint

Date: 2026-07-21
Status: accepted

mad-eye reads subscription usage from `GET https://api.anthropic.com/api/oauth/usage` — the same undocumented endpoint that powers Claude Code's `/usage` panel — authenticated with the OAuth access token from Claude Code's macOS Keychain item (`Claude Code-credentials`) plus the required headers (`anthropic-beta: oauth-2025-04-20`, `User-Agent: claude-code/<version>`).

There is no official or public API for subscription-limit percentages, so this is the only source. The trade-off: an undocumented endpoint (its URL, headers, and response shape) can change or break without notice, which would blind the app until the parsing is updated. Accepted because the feature is impossible otherwise; the response is parsed defensively (unknown/null windows are skipped, a 0–1-vs-0–100 scale guard is applied) to survive minor shape drift.
