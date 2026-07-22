# mad-eye

A macOS menubar app that watches your Claude subscription usage limits — a Mad-Eye Moody eye that gets more agitated as you approach a limit.

## Language

**Eye**: The menubar tray icon — an animated eye. It is the app's primary surface; there is no Dock presence. _Avoid_: tray icon, status item.

**Gauge**: One usage-limit reading: percent used plus the time until it resets. _Avoid_: bar, meter, limit (alone).

**Session Gauge**: The Gauge for the rolling ~5-hour session window ("Sesión actual" in the Claude usage panel).

**Weekly Gauge**: A Gauge for a 7-day rolling window — the all-models one, plus per-model ones (Fable, Sonnet).

**Driving Gauge**: The single Gauge that drives the Eye — **user-selectable** by clicking a Gauge in the Popover, and the choice **persists** across restarts. The default (no selection, or a selection that's no longer present) is the **Session Gauge**; with no Session Gauge, the maximum of the remaining Gauges. Its percent sets the Mood and triggers Shattered.

**Agitation**: How frantically the Eye moves — expressed as its Mood, derived from the Driving Gauge percent. The menubar itself never shows a number.

**Mood**: The Eye's discrete agitation level — **calm** · **nervous** · **paranoid** · **frantic** — a pure function of the Driving Gauge percent, escalating at rising thresholds. _Avoid_: state (reserved for the edge states below).

**Asleep**: The Eye state from launch until the first successful reading — the eye is closed.

**Stale**: The state after a TRANSIENT failure (rate limit, offline, server error) — the last gauges stay visible and the Eye holds its last Mood, with a warning in the Popover. Never closes the Eye. _Avoid_: outdated, cached.

**Blind**: The Eye state after an AUTH failure only (no credentials, or an expired token after a short grace) — the eye is closed; the Popover names the cause. A transient failure is Stale, not Blind.

**Shattered**: The Eye state when the Driving Gauge reaches 100% — the ring cracks and the eye freezes in a wide stare.

**Popover**: The panel opened by clicking the Eye, showing every Gauge in detail (the webview).

**Plan**: The Claude subscription tier whose limits the Gauges measure (e.g. Max 20x).

## Relationships

- The **Eye** has exactly one **Mood**, derived from the **Driving Gauge** percent.
- **Asleep**, **Blind**, and **Shattered** are Eye states that override the Mood.
- The **Popover** shows all **Gauges** — one Session Gauge + the Weekly Gauges — with the **Driving Gauge** highlighted.
- A **Plan** defines which **Gauges** exist and their limits.

## Flagged ambiguities

- _Resolved:_ Agitation follows the **Driving Gauge** — now user-selectable, defaulting to the **Session Gauge** (max-of-rest when there is no Session Gauge). It earlier defaulted to the Fable weekly Gauge; the default is now Session plus click-to-select.
