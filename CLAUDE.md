# mad-eye

## Product

A macOS menubar app showing Claude subscription usage limits (the /usage panel percentages) as an animated eye — Mad-Eye Moody style. The Eye gets more agitated as usage approaches a limit; clicking it opens a Popover with the detailed Gauges. Personal tool. Domain terms live in [CONTEXT.md](CONTEXT.md).

## Stack

- Tauri v2 — macOS only. Rust core owns the interesting parts: the Eye (tray icon + Agitation animation), usage fetching, Keychain access.
- Vanilla TypeScript + Vite for the Popover webview. No frontend framework.
- bun as package manager; `@kvnwolf/dobby` as the workflow dev dependency (gate + lifecycle + toolchain).
- Usage data: Anthropic's OAuth usage endpoint, authenticated with the credentials Claude Code stores in the macOS Keychain.

**Dev**: the web side runs via `dobby up` / `dobby dev` — dobby infers the dev command and wraps it in portless, so do NOT pin a dev command or hardcode a dev URL here. The NATIVE app (menubar Eye, real tray behavior) runs via `bun tauri dev`, which boots Vite itself on the fixed port 1420 that `src-tauri/tauri.conf.json` expects.

## Module map

- `src-tauri/` — Rust core: the Eye, Agitation, usage fetching, Keychain.
  - `src-tauri/src/eye/` — renders one Eye pose to a template tray icon ([CONTEXT](src-tauri/src/eye/CONTEXT.md)).
  - `src-tauri/src/tray/` — builds the menubar Eye and toggles/positions the Popover it opens ([CONTEXT](src-tauri/src/tray/CONTEXT.md)).
  - `src-tauri/src/usage/` — headless data core: fetch/parse the usage endpoint into Gauges, compute Driving Gauge + Mood + `Snapshot` ([CONTEXT](src-tauri/src/usage/CONTEXT.md)).
  - `src-tauri/src/keychain/` — read Claude Code's OAuth credentials from the Keychain, read-only ([CONTEXT](src-tauri/src/keychain/CONTEXT.md)).
- `src/` — the Popover webview (vanilla TS). `main.ts` bootstraps; `styles.css` is the dark panel theme.
  - `src/popover/` — the Popover panel: renders a `Snapshot` (Rust-mirrored type + `mockSnapshot` for the dev URL) ([CONTEXT](src/popover/CONTEXT.md)).

Each module gets its own CONTEXT.md (purpose · Files · Interface · Invariants · What's NOT here) as it is built.

## Conventions

- Organize by feature/domain — no type-based `components/` / `services/` / `lib/` buckets.
- No barrels: callers import by deep path; each file is named by its role (the filename is the interface).
- Co-locate the slice; inline by default; extract only on the second caller.
- Each module carries its own CONTEXT.md. What works for humans is also great for AI.
- Rust side follows the same philosophy: modules by domain (tray, usage, keychain), not by layer.

## Workflow config

- `/dobby:execute` runs `bunx dobby up` and reads the dev URL from `bunx dobby env` (portless-resolved, worktree-aware — never hardcode it). That URL serves the Popover UI in a browser for programmatic verification.
- Native behavior (menubar icon, Agitation, Keychain reads) is NOT reachable through the dev URL — verify it via `bun tauri dev` plus human/screenshot checks.
- The Rust side is gated by the `cargo check` extra in `dobby.config.json`; the inferred gate covers the TS side.
- Issue tracker: GitHub Issues (`tracker.type: "github"` in `dobby.config.json`).
