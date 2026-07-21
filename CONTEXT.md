# mad-eye

A macOS menubar app that watches your Claude subscription usage limits — a Mad-Eye Moody eye that gets more agitated as you approach a limit.

## Language

**Eye**: The menubar tray icon — an animated eye. It is the app's primary surface; there is no Dock presence. _Avoid_: tray icon, status item.

**Gauge**: One usage-limit reading: percent used plus the time until it resets. _Avoid_: bar, meter, limit (alone).

**Session Gauge**: The Gauge for the rolling ~5-hour session window ("Sesión actual" in the Claude usage panel).

**Weekly Gauge**: A Gauge for a 7-day rolling window. There are two: all-models, and the model-specific one (e.g. Fable/Opus).

**Agitation**: How frantically the Eye moves — a pure function of the highest Gauge percentage (calm when idle, frantic near a limit).

**Popover**: The panel opened by clicking the Eye, showing every Gauge in detail (the webview).

**Plan**: The Claude subscription tier whose limits the Gauges measure (e.g. Max 20x).

## Relationships

- The **Eye** has exactly one **Agitation** level, derived from the highest **Gauge** percent.
- The **Popover** shows all **Gauges** — one Session Gauge + the Weekly Gauges.
- A **Plan** defines which **Gauges** exist and their limits.

## Flagged ambiguities

- The usage endpoint is undocumented (OAuth credentials from Claude Code's macOS Keychain entry) — exact URL, response shape, and refresh cadence pending /dobby:research.
- Whether Agitation reacts to only the Session Gauge or the max across all Gauges — current assumption: max across all.
