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
- The two-Keychain-items gotcha: TWO items can share the `Claude Code-credentials` service (a stale one from an old login plus the live one). We read by account `$USER`, which selects the live item Claude Code keeps updating in place.
- Reading this item needs a per-app Keychain ACL grant ("Always Allow"), bound to the app's code signature. Every build must carry the stable signing identity (ADR 0004: cargo runner re-signs dev builds; `signingIdentity` covers bundles) or the prompt returns on the next 180s poll after a rebuild.
- Claude Code REWRITES this item on every token refresh (~hourly), which resets the item's ACL and evicts mad-eye — so each read after a refresh re-prompts, no matter how mad-eye is signed. `lib.rs` mitigates by caching the read (`AppState.cached_credentials`) and only re-reading when the token nears its `expires_at`, cutting reads from every 180s to ~once per token lifetime. It cannot be eliminated from this side (the item is Claude Code's); the real fix is mad-eye owning its own OAuth token — a separate, unbuilt slice.
- The token VALUE is never logged or exposed beyond the fetch layer.
- Parses DEFENSIVELY: only `accessToken` is required; `expiresAt` / `subscriptionType` are best-effort (the blob shape varies by Claude Code version).
- The `security-framework` crate is a hidden adapter — the interface is one function; a CLI shell-out could replace it without callers changing.

## What's intentionally NOT here

- No token refresh / write path (deferred; delegated-CLI refresh is a later slice).
- No mapping of the Keychain result to app state — the poll loop in `lib.rs` turns a `KeychainError` into a Blind Snapshot.
- No pure-parse seam exposed (`read_claude_credentials` reaches security-framework internally); the defensive parse is not separately unit-tested.
