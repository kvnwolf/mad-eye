# 0001. Tauri over SwiftUI for a macOS-only menubar app

Date: 2026-07-20
Status: accepted

## Context

mad-eye targets macOS only. The default native choice would be SwiftUI/AppKit (`NSStatusItem`); Tauri brings a Rust core plus a web-rendered panel, at the cost of a bundled webview and a heavier toolchain for a very small UI.

## Decision

Tauri v2: Rust owns the core (tray icon, Agitation animation, OAuth usage fetch, Keychain); web tech renders only the small Popover.

## Consequences

- Stays inside the author's stack (TypeScript + the dobby kit) with no Xcode project to maintain; Rust gets the interesting parts.
- Tray animation goes through Tauri's TrayIcon icon-swapping API rather than native NSView drawing — frame-based, so CPU wakeups need care.
- If pixel-perfect native behavior (e.g. true NSPopover) ever becomes a requirement, that pressure revisits this decision.
