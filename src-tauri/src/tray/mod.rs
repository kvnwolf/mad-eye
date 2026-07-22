//! The menubar Eye: the app's tray icon, and the owner of the Popover it opens.
//!
//! A left-click on the Eye toggles a hidden, borderless, always-on-top Popover
//! window (declared in `tauri.conf.json`), positioning it under the tray icon.
//! The Popover hides on blur (wired in `lib.rs` via `on_window_event`). Because
//! macOS fires that blur an instant BEFORE the tray click that caused it, a
//! naive toggle would blur-hide then click-show and the Popover would never
//! close — so we stamp the last-hide time ([`PopoverGuard`]) and ignore any
//! open that lands within [`REOPEN_GUARD`] of a hide.
//!
//! The tray HAS a right-click context menu — "Launch at login" (a live checkmark
//! reflecting the autostart plugin) and "Quit". Left-click opens the Popover;
//! Refresh lives in the Popover, NOT the menu. Opening the Popover never triggers
//! a fetch — usage refreshes only via the 180s poll and the Popover's Refresh
//! control (opening the panel used to fetch, which rapid reopen could spam into a
//! 429 → Blind). `show_menu_on_left_click(false)` keeps the two gestures separate:
//! right-click → menu, left-click → the Popover toggle.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::menu::{CheckMenuItem, MenuBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Rect, WebviewWindow};
use tauri_plugin_autostart::ManagerExt;

use crate::eye::render::{render_frame, EyeFrame};

/// The Popover window's label — must match `tauri.conf.json` and the capability.
const POPOVER_LABEL: &str = "popover";

/// How long after a hide a tray click is treated as "the click that caused the
/// blur" and ignored, so the blur-then-click sequence doesn't double-toggle the
/// Popover back open. ~150ms is well above the blur→click gap, well below a
/// deliberate reopen.
const REOPEN_GUARD: Duration = Duration::from_millis(150);

/// The race guard's single piece of state: when the Popover was last hidden.
/// `None` until the first hide, so the first-ever open is never guard-blocked.
pub struct PopoverGuard {
    last_hidden: Mutex<Option<Instant>>,
}

impl PopoverGuard {
    fn new() -> Self {
        PopoverGuard {
            last_hidden: Mutex::new(None),
        }
    }
}

/// Build the menubar Eye as a static, open, centred variant-D template icon, wire
/// its left-click to toggle the Popover, and attach a right-click context menu
/// (Launch at login + Quit). `show_menu_on_left_click(false)` keeps the menu on
/// right-click only, so left-click stays the Popover toggle. Refresh is NOT in the
/// menu — it lives in the Popover.
///
/// `icon_as_template(true)` lets macOS tint it for light/dark menu bars. Also
/// manages the [`PopoverGuard`] so the click/blur handlers can reach it.
pub fn build_tray(app: &AppHandle) -> tauri::Result<TrayIcon> {
    app.manage(PopoverGuard::new());

    // Right-click menu: a "Launch at login" checkmark seeded from the autostart
    // plugin's real state, a separator, then "Quit". Keep a clone of the check
    // item so the menu-event handler can flip its checkmark after toggling.
    let launch = CheckMenuItem::with_id(
        app,
        "launch",
        "Launch at login",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = MenuBuilder::new(app)
        .item(&launch)
        .item(&separator)
        .item(&quit)
        .build()?;
    let launch_check = launch.clone();

    TrayIconBuilder::with_id("eye")
        .icon(render_frame(&EyeFrame::static_open()))
        .icon_as_template(true)
        .menu(&menu)
        // Right-click → menu; left-click → the Popover toggle below (not the menu).
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            // Flip autostart and mirror the new state onto the checkmark. The
            // plugin is the source of truth — re-read it, don't assume the set.
            "launch" => {
                let al = app.autolaunch();
                if al.is_enabled().unwrap_or(false) {
                    let _ = al.disable();
                    let _ = launch_check.set_checked(false);
                } else {
                    let _ = al.enable();
                    let _ = launch_check.set_checked(true);
                }
            }
            // Explicit Quit: set the clean-quit flag so `lib.rs`'s run handler lets
            // this exit through (it otherwise prevents exit to stay window-less).
            "quit" => {
                crate::QUIT.store(true, std::sync::atomic::Ordering::SeqCst);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Toggle on a completed left-click (button released), the natural
            // "clicked the menubar item" gesture. `rect` locates the icon.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_popover(tray.app_handle(), &rect);
            }
        })
        .build(app)
}

/// Left-click behaviour: if the Popover is up, take it down; otherwise position
/// it under the Eye and show it — unless the guard says this click is really the
/// one that just blur-hid it. Opening deliberately does NOT fetch: usage refreshes
/// only via the 180s poll and the Popover's Refresh control, so rapidly reopening
/// the panel can't spam the endpoint into a rate-limit (429 → Blind).
fn toggle_popover(app: &AppHandle, rect: &Rect) {
    let Some(window) = app.get_webview_window(POPOVER_LABEL) else {
        return;
    };

    if window.is_visible().unwrap_or(false) {
        hide_popover(app);
        return;
    }

    // The Popover is hidden. If it was hidden just now, this click is the tail of
    // a blur-then-click close — swallow it instead of re-opening.
    if recently_hidden(app) {
        return;
    }

    position_popover(&window, rect);
    let _ = window.show();
    let _ = window.set_focus();
}

/// Hide the Popover and stamp the last-hide time. Called by the tray toggle and
/// by the `on_window_event` blur handler in `lib.rs`; both routes must stamp so
/// the guard covers hide-by-blur as well as hide-by-click.
pub fn hide_popover(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(POPOVER_LABEL) {
        let _ = window.hide();
    }
    if let Some(guard) = app.try_state::<PopoverGuard>() {
        *guard.last_hidden.lock().unwrap() = Some(Instant::now());
    }
}

/// True if the Popover was hidden within the last [`REOPEN_GUARD`].
fn recently_hidden(app: &AppHandle) -> bool {
    app.try_state::<PopoverGuard>()
        .and_then(|guard| *guard.last_hidden.lock().unwrap())
        .is_some_and(|hidden| hidden.elapsed() < REOPEN_GUARD)
}

/// Place the Popover under the tray icon: horizontally centred on the icon
/// (clamped so it never spills off the monitor) and dropped just below it. All
/// maths is in physical pixels — the tray `rect` and the window size are both
/// converted with the window's scale factor.
fn position_popover(window: &WebviewWindow, rect: &Rect) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let tray_pos = rect.position.to_physical::<f64>(scale);
    let tray_size = rect.size.to_physical::<f64>(scale);
    let win_size = window
        .outer_size()
        .unwrap_or_else(|_| PhysicalSize::new(0, 0));

    let center_x = tray_pos.x + tray_size.width / 2.0;
    let mut x = center_x - win_size.width as f64 / 2.0;
    // Drop the Popover just below the icon / menubar.
    let y = tray_pos.y + tray_size.height;

    // Clamp X to the monitor so a tray icon near a screen edge doesn't push the
    // Popover off-screen.
    if let Ok(Some(monitor)) = window.current_monitor() {
        let min_x = monitor.position().x as f64;
        let max_x = min_x + monitor.size().width as f64 - win_size.width as f64;
        x = x.clamp(min_x, max_x.max(min_x));
    }

    let _ = window.set_position(PhysicalPosition::new(x as i32, y as i32));
}
