# usage

The headless data core: turn Claude's OAuth usage endpoint into the `Snapshot`
the Eye and Popover read.

## Files

- `types.rs` — the wire types: `Mood`, `GaugeKind`, `Gauge`, `Status`, `Snapshot` (+ `Snapshot::asleep()`). These serde attributes ARE the seam with the TS Popover.
- `mood.rs` — pure logic: `mood_for(pct)` (Mood bands) and `driving_gauge(&[Gauge], selected)` (which Gauge drives the Eye — a user selection by name, else the Session default). No IO — the tested contract.
- `client.rs` — the endpoint adapter: `parse_usage(json)` (pure — maps the API's `limits[]` by `kind`, with a legacy-window fallback) and `fetch_usage(token)` (ureq GET + header + status mapping), plus `FetchError`.
- `mod.rs` — module declarations (no barrel; import by deep path).

## Interface

- `Snapshot { plan, gauges, driving_idx, mood, status, status_note, retry_at, fetched_at }` — serialized camelCase; the single value the Popover renders. `retry_at` is the epoch-ms a rate-limited retry is allowed (429 `Retry-After`), driving the Popover countdown + poll backoff. `Snapshot::asleep()` is the startup state.
- `Gauge { name, kind, utilization, resets_at }` — one usage window, `utilization` normalized to 0–100.
- `Mood { Calm, Nervous, Paranoid, Frantic, Shattered }` (lowercase wire) · `GaugeKind { Session, WeeklyAll, WeeklyScoped }` (camelCase wire: `session`/`weeklyAll`/`weeklyScoped`, mirroring the API's `limits[].kind`; the model name lives in `Gauge.name`) · `Status { Normal, Stale, Blind, Asleep }` (lowercase wire).
- `mood_for(pct: f64) -> Mood` · `driving_gauge(gauges: &[Gauge], selected: Option<&str>) -> Option<usize>` · `parse_usage(json: &str) -> Result<Vec<Gauge>, FetchError>` · `fetch_usage(token: &str) -> Result<Vec<Gauge>, FetchError>`.
- `FetchError { Unauthorized, RateLimited(Option<u64>), Http(u16), Network(String), Parse(String) }` — `RateLimited` carries the `Retry-After` delta-seconds when the 429 sends one.

## Invariants

- Mood bands (Decision #3): `<50` Calm · `<80` Nervous · `<95` Paranoid · `<100` Frantic · `>=100` Shattered (clamp above 100). Pure function of the driving %.
- Driving Gauge (Decision #2, revised): selection-aware. A user selection wins — if `selected` names a present Gauge, that index; else the **Session** Gauge (`GaugeKind::Session`) is the default; with no Session Gauge, the max-utilization Gauge among all; `None` if empty. (The old WeeklyScoped-first default is gone.) `lib.rs` owns the selection (persisted `selected_driver`) and threads it in; the Popover's `set_driver` command changes it and recomputes the Snapshot's `driving_idx` + `mood` with no fetch.
- Scale: `limits[].percent` is authoritative (already 0–100) — NO scale guard. The ×100 guard (a `utilization <= 1.0` is a 0..1 fraction; boundary `1.0` inclusive → 100.0) applies ONLY to the legacy top-level windows in the fallback.
- `limits[]` → gauge mapping and output ORDER are fixed: `kind:"session"`→Session, `"weekly_all"`→Weekly · all models, `"weekly_scoped"`→`Weekly · {scope.model.display_name | "model"}` (WeeklyScoped); unknown kinds are SKIPPED (forward-compatible), `limits[]` order preserved. Legacy fallback (only when `limits` is empty/absent): `five_hour`→Session, `seven_day`→Weekly · all models, `seven_day_opus`→Weekly · Fable, `seven_day_sonnet`→Weekly · Sonnet (both scoped windows → `WeeklyScoped`), null windows skipped.
- `fetch_usage` sends exactly the four required headers and maps 401→Unauthorized, 429→RateLimited before parsing. The token is only ever a header — NEVER logged.
- The camelCase/lowercase serde renames are the TS Popover contract (`src/popover/snapshot.ts`) — do not change without updating both sides.

## What's intentionally NOT here

- No token refresh — 401 is surfaced as `Unauthorized`, never retried with a new token (Decision #4).
- No Keychain access — reading Claude Code's OAuth credentials (and the two-Keychain-items gotcha) is the `keychain` module's concern; this module only takes a `token` string.
- No polling, threading, or AppState wiring (that lives in `lib.rs`).
- No `extra_usage` / pay-as-you-go Gauge yet (flagged in research).
- No HTTP-status unit seam: `fetch_usage` builds ureq internally; the status→error mapping is verified live, `parse_usage` covers the body half.
