# keychain

Read Claude Code's OAuth credentials from the macOS Keychain — read-only, one
function, security-framework hidden behind it.

## Files

- `read.rs` — `read_claude_credentials() -> Result<Credentials, KeychainError>`: reads the `Claude Code-credentials` item (account `$USER`) via `security-framework` and defensively parses the `claudeAiOauth` blob.
- `mod.rs` — module declaration only (no barrel; import `read` by deep path).

## Interface

- `read_claude_credentials() -> Result<Credentials, KeychainError>` — the sole entry point.
- `Credentials { access_token: String, expires_at: Option<i64>, subscription_type: Option<String> }` — parsed from the nested `claudeAiOauth` object.
- `KeychainError { NotFound, Denied, Parse(String) }` — every variant maps to a Blind Snapshot upstream.

## Invariants

- READ-ONLY. Never writes or refreshes the token (Decision #4 — rotation would break Claude Code's own login).
- The token VALUE is never logged or exposed beyond the fetch layer.
- Parses DEFENSIVELY: only `accessToken` is required; `expiresAt` / `subscriptionType` are best-effort (the blob shape varies by Claude Code version).
- The `security-framework` crate is a hidden adapter — the interface is one function; a CLI shell-out could replace it without callers changing.

## What's intentionally NOT here

- No token refresh / write path (deferred; delegated-CLI refresh is a later slice).
- No mapping of the Keychain result to app state — the poll loop in `lib.rs` turns a `KeychainError` into a Blind Snapshot.
- No pure-parse seam exposed (`read_claude_credentials` reaches security-framework internally); the defensive parse is not separately unit-tested.
