# tray

The menubar Eye: creates the macOS tray icon, and owns the Popover it toggles.

## Files

- `mod.rs` — `build_tray(app)`: builds the tray (id `"eye"`, a static rendered Eye frame, template icon), attaches a right-click context menu (Launch at login + Quit) and routes its events, and wires left-click to toggle the Popover; plus the Popover show/hide/position logic and the blur-vs-click race guard. Refresh is NOT in the menu — it lives in the Popover.

## Interface

- `build_tray(app: &AppHandle) -> tauri::Result<TrayIcon>` — call once from `lib.rs` `.setup()`. Builds the right-click menu (Launch at login + Quit) and its `on_menu_event` router, wires left-click to the Popover toggle, and manages the `PopoverGuard` state. Reach the built icon later via `AppHandle::tray_by_id("eye")`.
- `hide_popover(app: &AppHandle)` — hide the Popover and stamp the last-hide time. Called by the tray toggle and by `lib.rs`'s `on_window_event` blur handler (hide-on-blur).
- `PopoverGuard` — managed state holding the last-hide `Instant`; drives the reopen race guard. Constructed and managed by `build_tray`.

## Invariants

- Tray id is `"eye"` — the animator and Popover wiring look it up by this id.
- Icon is set with `icon_as_template(true)` and MUST stay a pure-black template (see `eye/`) for correct light/dark tinting.
- The Popover window label is `"popover"` — MUST match `tauri.conf.json` and `capabilities/default.json`.
- Both hide routes (tray toggle + blur) MUST go through `hide_popover` so every hide stamps the guard; otherwise the blur-then-click sequence double-toggles the Popover back open.
- The reopen guard (`REOPEN_GUARD`, ~150ms) must stay above the macOS blur→click gap and below a deliberate reopen.
- Left-click toggles: visible → `hide_popover`; hidden → position from the tray `rect` + `show()` + `set_focus()`. It MUST NOT fetch — opening the Popover used to spawn `crate::refresh_now`, and rapidly reopening could spam the endpoint into a 429 (→ Blind). Usage now refreshes ONLY via the 180s poll and the Popover's Refresh control.
- The tray HAS a right-click context menu: `MenuBuilder` assembles a `"launch"` `CheckMenuItem` + separator + `"quit"` `MenuItem`, attached with `.menu(&menu)` + `.show_menu_on_left_click(false)` so right-click shows the menu and left-click stays the Popover toggle. `on_menu_event` routes `"launch"` (flip autolaunch via the autostart plugin, then mirror the new state onto a cloned `CheckMenuItem`'s checkmark) and `"quit"` (set `crate::QUIT`, then `app.exit(0)`). Refresh is NOT in the menu — it is a button INSIDE the Popover that invokes `refresh_now_cmd` (wired by the delegated click handler in `src/main.ts`).
- The Popover window is transparent + frosted (`tauri.conf.json` `transparent:true` + `macOSPrivateApi`; vibrancy applied in `lib.rs` setup) and is sized to its content by the webview (`src/main.ts` `setSize`). `position_popover` therefore anchors by the TOP edge (`y = tray_rect.bottom`, X centred on the icon and clamped to the monitor): a content resize keeps the top-left fixed (tao) so the panel stays just under the Eye and grows downward. It reads `outer_size()` at click time, which already reflects the latest `setSize`.

## What's intentionally NOT here

- No `get_snapshot` command or `snapshot-updated` emit — the Rust↔Popover data seam lives in `lib.rs`.
- No usage-data command — `refresh_now_cmd` is a `#[tauri::command]` in `lib.rs`; the Popover's Refresh button invokes it by `data-action`. This module owns the tray icon, its right-click menu, and the Popover window's show/hide/position — not the fetch.
- No `on_window_event` registration — that is wired on the builder in `lib.rs`; this module only exposes `hide_popover`.
- No Popover rendering — that is the TS side (`src/popover/`); Rust only positions and shows/hides the window.
- No autostart-plugin registration or the on-by-default enable — those live in `lib.rs` `.setup()`. The tray's "Launch at login" menu item, however, DOES read (to seed its checkmark) and toggle the autolaunch state at runtime via the plugin's `ManagerExt` (`app.autolaunch()`).
- No frosted-glass / vibrancy setup — the transparent window is declared in `tauri.conf.json` and `apply_vibrancy` (window_vibrancy) is called in `lib.rs` `.setup()`. This module only positions/shows/hides the window.
- No window content-sizing — the webview owns that: `src/main.ts` measures the card and calls `setSize` after every render (needs `core:window:allow-set-size` in `capabilities/default.json`).
- No `QUIT` flag definition — it is a `pub static` in `lib.rs`; the tray menu's `"quit"` handler sets it before `app.exit(0)` so `lib.rs`'s run handler lets that exit through.
