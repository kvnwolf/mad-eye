# Popover

The webview panel opened by clicking the Eye — renders every Gauge in detail and carries the Refresh control. It draws from a `Snapshot` (usage data) plus a small `PopoverOptions` (app state — the `refreshing` flag), so the whole UI is verifiable in a plain browser (no Tauri) at the dobby dev URL via `?state=`. Launch at login and Quit live in the tray's right-click menu, not here.

## Files

- `snapshot.ts` — the `Snapshot` type (mirrors Rust `usage/types.rs`, camelCased), `mockSnapshot(state)` (representative data for each Status, keyed off `?state=`; the driving Gauge defaults to the Session gauge, index 0), and `mockMoodFor(pct)` (a mirror of Rust `mood_for` for the dev-URL selection demo). Pure usage data — it carries NO control state. `plan` is still in the type (Rust still serializes it) but is no longer rendered.
- `render.ts` — `renderPopover(root, snap, opts?)`: builds the dark/light panel from a Snapshot (no plan header — it starts at the gauges), appends the always-present footer (an "updated ago" line + a Refresh button, `data-action="refresh"`, whose icon is a centred inline SVG circular-arrow), and runs the live "resets in / updated ago" countdown. Each gauge row is a click/keyboard target (`data-action="select-driver"` + `data-name`, `role="button"`, `tabindex="0"`) that selects it as the Eye's driver; the driving row wears the accent + a "👁 watching" badge, the rest a hover/focus-revealed "track" hint. When `opts.refreshing` the icon spins and the button is disabled. Exports `PopoverOptions`.
- `../main.ts` — bootstrap: real app (Tauri present) vs dev URL (`?state=`); mounts into `#app`. On the Tauri path it holds a `refreshing` flag + the last `current` Snapshot, wires the Refresh button AND the gauge rows with delegated click/keydown listeners on the stable root (Refresh → `refresh_now_cmd`, spinner off on `snapshot-updated` with a ~10s timeout fallback; a gauge → `invoke('set_driver', { name })`, whose emitted `snapshot-updated` repaints), and sizes the OS window to the card after every render (`setSize`). On the dev-URL path there is no Tauri: selecting a gauge updates the mock Snapshot locally (`mockMoodFor` recomputes the Mood) and re-renders, so the interaction is demonstrable in a plain browser.
- `../styles.css` — the panel's styling: a translucent card that fills the transparent, macOS-vibrant window edge-to-edge. Dark by default; a `prefers-color-scheme: light` block flips the theme variables for legibility on the light frost. Flex-centres the Refresh icon and spins it (`@keyframes madeye-spin`, respecting `prefers-reduced-motion`) while refreshing.

## Interface

- `renderPopover(root: HTMLElement, snap: Snapshot, opts?: PopoverOptions): void` — the seam. Idempotent: call again with a new Snapshot (and/or opts) to re-render; the previous countdown interval is cleared first.
- `PopoverOptions { refreshing?: boolean }` — non-Snapshot render input (app state). When true, the Refresh button spins and is disabled (`aria-busy`).
- `mockSnapshot(state: Status): Snapshot` — dev/mock data.
- Types: `Snapshot`, `Gauge`, `GaugeKind`, `Mood`, `Status`, `PopoverOptions`.
- Control contract: the Refresh button carries `data-action="refresh"` and each gauge row carries `data-action="select-driver"` + `data-name="<gauge name>"`; the host (`main.ts`) dispatches on both (Refresh → `refresh_now_cmd`, a gauge → `set_driver`). Launch at login + Quit are in the tray's right-click menu.

## Invariants

- The Popover reads its usage from a `Snapshot` and its control state from `PopoverOptions` — no fetching, no Keychain, no Mood computation here. Live Snapshots arrive over IPC through `main.ts`; the `refreshing` flag is app state held in `main.ts` (not a Snapshot field), so `Snapshot` stays a pure mirror of the Rust usage type.
- All user-facing strings are English (Decision #7).
- The footer (an "updated ago" line + the Refresh button) renders in EVERY state (normal, stale, blind, asleep). Refresh is the ONLY in-Popover control — Launch at login and Quit are in the tray's right-click menu. The `asleep` DATA area is still skeleton bars ONLY (no gauge names, no numbers); the footer is chrome, not data.
- The Refresh button is wired by ONE delegated listener on the stable `root` in `main.ts` (attached ONLY under Tauri). renderPopover replaces `root`'s children on every re-render, so a per-element handler would be lost — delegation on the unchanged parent survives. In a plain browser the button renders but is inert (no listener, no `invoke`).
- The refresh spinner is driven by `opts.refreshing`, a `main.ts` flag: a click sets it true and repaints (spinner on) → `invoke('refresh_now_cmd')`; the resulting `snapshot-updated` clears it and repaints (back to the static icon). A ~10s timeout fallback clears it if no snapshot arrives, so a hung fetch can't leave the spinner stuck. Repeat clicks are ignored while already refreshing.
- The driving Gauge (`drivingIdx`) is the one that moves the Eye; it wears the amber accent + a "👁 watching" badge. It is now USER-SELECTABLE: clicking/activating any gauge row makes it the driver (Rust `set_driver` persists the choice and re-emits the Snapshot; the dev-URL path mimics this locally). The default driver is the Session gauge.
- `Snapshot`'s shape must stay in lockstep with the Rust `Snapshot` (camelCase mirror). Control state is deliberately NOT a Snapshot field.
- Re-render must not leak the countdown interval (one timer per root, cleared up front).
- The window is transparent and frosted by macOS vibrancy (set up in Rust); the card MUST stay translucent (never an opaque page/card background) and fill the window edge-to-edge, or the black-rectangle look returns. Card radius (12px) matches the vibrancy corner radius. Theme differences live in CSS variables (the `:root` block + its `prefers-color-scheme: light` override) so the card is legible in both light and dark.
- Only the Tauri path resizes the window (`main.ts` `setSize`, guarded by the `__TAURI__` check); the dev-URL browser path never calls it. The `.popover` element is always present after a render (normal/stale/blind/skeleton all render a `section.popover` that also contains the footer), so it is the height-measurement anchor and `setSize` measures the FULL card including the footer. The card is shorter now that Refresh is the only control, so `setSize` MUST re-measure on each paint. Sizing needs `core:window:allow-set-size` in `capabilities/default.json`.

## What's intentionally NOT here

- No fetching, Keychain, or Mood computation — those live on the Rust side.
- No control command definitions — `refresh_now_cmd` and `set_driver` are Rust `#[tauri::command]`s in `lib.rs`; this module only emits `data-action="refresh"` / `data-action="select-driver"` and `main.ts` invokes them. Launch at login and Quit are NOT commands — they are tray-menu items handled entirely in Rust (`src-tauri/src/tray/`).
- No framework and no barrel `index.ts` — vanilla TS, deep-path imports only.
- No click-to-open / outside-click-close / positioning — the tray (`src-tauri/src/tray/`) owns window behavior.
- No frosted-glass setup — the transparent window is declared in `tauri.conf.json` and the macOS vibrancy is applied in Rust (`lib.rs` setup). This module only renders the translucent card and asks the OS window to match its height.
