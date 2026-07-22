# 0003. mad-eye never refreshes the OAuth token — it goes Blind instead

Date: 2026-07-21
Status: accepted

mad-eye only READS the access token from Claude Code's Keychain item. When the token is expired or a request returns 401, the Eye enters the **Blind** state and waits for Claude Code to refresh the token on its own next use — mad-eye never calls the refresh endpoint itself.

Anthropic's OAuth refresh tokens are single-use / rotating: refreshing invalidates the previous refresh token. If mad-eye refreshed independently, Claude Code's next refresh would fail and force the user to re-login (and vice-versa). The trade-off is liveness — the Eye can be Blind for up to the token's ~8h lifetime if Claude Code is never used — in exchange for never breaking Claude Code's session. A future slice could delegate refresh to the `claude` CLI (the CodexBar pattern) to keep the Eye live without touching the token directly.
