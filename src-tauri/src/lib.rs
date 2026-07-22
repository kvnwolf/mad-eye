pub mod eye;
mod keychain;
mod tray;
pub mod usage;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

/// Clean-quit flag. The run handler prevents exit on EVERY `ExitRequested` so a
/// window close (blur-hidden Popover, Cmd-Q on the Popover) never kills this
/// tray-only app. An explicit Quit from the tray menu sets this true immediately
/// before `app.exit(0)`, and the run handler then lets that exit through. Kept as
/// a `static` so the tray menu handler (`tray::mod`) and the run handler can both
/// reach it without threading extra state through.
pub static QUIT: AtomicBool = AtomicBool::new(false);

use keychain::read::read_claude_credentials;
use usage::client::{fetch_usage, FetchError};
use usage::mood::{driving_gauge, mood_for};
use usage::types::{Snapshot, Status};

/// Poll floor (Decision / Spec): never fetch more often than this to stay under
/// the endpoint's rate limit.
const POLL_INTERVAL: Duration = Duration::from_secs(180);

/// Consecutive failures before the Eye goes Blind (Decision #9).
const BLIND_AFTER_FAILURES: u32 = 3;

/// The app's shared state: the latest Snapshot the Popover reads, and the count
/// of consecutive failed fetches that drives the Blind threshold.
pub struct AppState {
    pub snapshot: Mutex<Snapshot>,
    pub fail_count: Mutex<u32>,
    /// The user's Driving-Gauge selection: the chosen Gauge NAME, or `None` for
    /// the default (Session). Persisted to `driver.json` and threaded into
    /// `driving_gauge` on every refresh. Seeded from disk at startup.
    pub selected_driver: Mutex<Option<String>>,
}

/// The Popover's read side of the seam: hand back a clone of the latest
/// Snapshot so the webview can render immediately on open (Decision #8). Live
/// updates arrive separately via the `snapshot-updated` event.
#[tauri::command]
fn get_snapshot(state: tauri::State<AppState>) -> Snapshot {
    state.snapshot.lock().unwrap().clone()
}

/// The Popover's manual "Refresh" control: kick a fetch off the calling thread
/// so the IPC returns at once; the resulting `snapshot-updated` event re-renders
/// the panel. This command and the 180s poll are now the ONLY refresh paths —
/// opening the Popover no longer fetches (that could be spammed into a 429).
#[tauri::command]
fn refresh_now_cmd(app: AppHandle) {
    thread::spawn(move || refresh_now(&app));
}

/// The Popover's "make this Gauge drive the Eye" control. Sets the selection (the
/// Gauge NAME, or `None` to reset to the Session default), PERSISTS it, then — with
/// NO network fetch — recomputes the currently-stored Snapshot's `driving_idx` and
/// `mood` from its existing gauges under the new selection, stores it back, and
/// emits `snapshot-updated` so the Eye and the Popover update instantly.
#[tauri::command]
fn set_driver(app: AppHandle, state: tauri::State<AppState>, name: Option<String>) {
    *state.selected_driver.lock().unwrap() = name.clone();
    save_driver(&app, &name);

    let snapshot = {
        let mut snapshot = state.snapshot.lock().unwrap();
        let driving_idx = driving_gauge(&snapshot.gauges, name.as_deref());
        let mood = driving_idx.map(|idx| mood_for(snapshot.gauges[idx].utilization));
        snapshot.driving_idx = driving_idx;
        snapshot.mood = mood;
        snapshot.clone()
    };

    let _ = app.emit("snapshot-updated", &snapshot);
}

/// The on-disk shape of the persisted Driving-Gauge selection: `{ "driver": <name> | null }`.
#[derive(Serialize, Deserialize)]
struct DriverConfig {
    driver: Option<String>,
}

/// The file (in the app config dir) that persists the Driving-Gauge selection.
const DRIVER_FILE: &str = "driver.json";

/// Read the persisted Driving-Gauge selection (a Gauge NAME) from the app config
/// dir. Tolerant: a missing dir/file or corrupt JSON reads as `None` (the default).
fn load_driver(app: &AppHandle) -> Option<String> {
    let dir = app.path().app_config_dir().ok()?;
    let contents = std::fs::read_to_string(dir.join(DRIVER_FILE)).ok()?;
    serde_json::from_str::<DriverConfig>(&contents)
        .ok()
        .and_then(|config| config.driver)
}

/// Persist the Driving-Gauge selection (best-effort): create the app config dir if
/// missing, then write `{ "driver": <name> | null }`. Any IO error is ignored — a
/// failed save just means the choice is not remembered next launch.
fn save_driver(app: &AppHandle, driver: &Option<String>) {
    let Ok(dir) = app.path().app_config_dir() else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let config = DriverConfig {
        driver: driver.clone(),
    };
    if let Ok(json) = serde_json::to_string(&config) {
        let _ = std::fs::write(dir.join(DRIVER_FILE), json);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            refresh_now_cmd,
            set_driver
        ])
        // Hide-on-blur: whenever the Popover loses focus (a click elsewhere, the
        // tray, Cmd-Tab), tuck it away and stamp the last-hide time so the very
        // next tray click doesn't immediately re-open it (see `tray`'s guard).
        .on_window_event(|window, event| {
            if window.label() == "popover" {
                if let WindowEvent::Focused(false) = event {
                    tray::hide_popover(window.app_handle());
                }
            }
        })
        .setup(|app| {
            // Ghost app: no Dock icon, no ⌘-Tab presence.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Launch-at-login: register the autostart plugin (macOS LaunchAgent —
            // the documented best practice) and turn it ON by default on the first
            // run. The tray's right-click "Launch at login" menu item reads and
            // flips this state at runtime (see `tray::build_tray`).
            app.handle().plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                None,
            ))?;
            {
                use tauri_plugin_autostart::ManagerExt;
                let al = app.autolaunch();
                if !al.is_enabled().unwrap_or(false) {
                    let _ = al.enable();
                }
            }

            tray::build_tray(app.handle())?;

            // Native frosted-glass Popover. The window is transparent (see
            // `tauri.conf.json`: `transparent` + `macOSPrivateApi`); here we lay a
            // macOS vibrancy view behind the webview so the panel reads as a real
            // menubar popover — translucent, rounded, with the OS drop-shadow —
            // instead of the opaque black rectangle it was. `Popover` is the
            // semantic menubar-popover material; the corner radius matches the
            // card's CSS radius (12px). The webview then sizes the OS window to the
            // card (see `src/main.ts`) so there is no empty frosted margin.
            #[cfg(target_os = "macos")]
            {
                use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
                if let Some(popover) = app.get_webview_window("popover") {
                    let _ = apply_vibrancy(
                        &popover,
                        NSVisualEffectMaterial::Popover,
                        Some(NSVisualEffectState::Active),
                        Some(12.0),
                    );
                }
            }

            // Startup state: the Eye is asleep until the first successful fetch.
            // Seed the Driving-Gauge selection from disk so a prior choice persists.
            let selected_driver = load_driver(app.handle());
            app.manage(AppState {
                snapshot: Mutex::new(Snapshot::asleep()),
                fail_count: Mutex::new(0),
                selected_driver: Mutex::new(selected_driver),
            });

            // Headless poll loop: first tick immediate, then every 180s.
            let handle = app.handle().clone();
            thread::spawn(move || poll_loop(handle));

            // Animate the Eye: a background thread reads AppState.snapshot each
            // cycle and darts the pupil by Mood. Managed so it outlives setup.
            let animator = eye::animator::Animator::spawn(app.handle().clone());
            app.manage(animator);

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // Zero-window tray-only app: keep it alive when nothing asks to exit —
    // EXCEPT an explicit Quit from the tray menu, which sets `QUIT` first so this
    // handler lets the exit through instead of trapping it.
    app.run(|_app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !QUIT.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
    });
}

/// The poll: an immediate first tick, then `refresh_now` every interval — except
/// after a rate-limit, where it backs off until the server's Retry-After window.
fn poll_loop(handle: AppHandle) {
    loop {
        refresh_now(&handle);
        thread::sleep(next_poll_delay(&handle));
    }
}

/// How long to wait before the next automatic poll. Normally `POLL_INTERVAL`;
/// when the last poll was rate-limited with a `retry_at`, wait until that window
/// passes (+2s buffer) instead of hammering early — clamped to 5s..1h so a bogus
/// value can neither busy-loop nor stall the poll forever.
fn next_poll_delay(app: &AppHandle) -> Duration {
    let retry_at = app.state::<AppState>().snapshot.lock().unwrap().retry_at;
    match retry_at {
        Some(retry_at) => {
            let remaining = (retry_at - now_millis()).max(0) as u64 + 2_000;
            Duration::from_millis(remaining.clamp(5_000, 3_600_000))
        }
        None => POLL_INTERVAL,
    }
}

/// One full refresh: read creds → fetch → fold into a new Snapshot → store it in
/// AppState → emit `snapshot-updated` for any open Popover. The blocking Keychain
/// read and network fetch happen OUTSIDE any lock. Shared by the 180s loop and
/// the fetch-on-open path (a tray click spawns this on its own thread), so it
/// must stay self-contained and safe to run concurrently. Emits AFTER storing so
/// a live listener never sees an event newer than AppState.
pub fn refresh_now(app: &AppHandle) {
    let (last, mut fail_count, selected_driver) = {
        let state = app.state::<AppState>();
        let snapshot = state.snapshot.lock().unwrap().clone();
        let fail_count = *state.fail_count.lock().unwrap();
        let selected_driver = state.selected_driver.lock().unwrap().clone();
        (snapshot, fail_count, selected_driver)
    };

    let snapshot = poll_once(&last, &mut fail_count, selected_driver.as_deref());
    log_snapshot(&snapshot);

    {
        let state = app.state::<AppState>();
        *state.snapshot.lock().unwrap() = snapshot.clone();
        *state.fail_count.lock().unwrap() = fail_count;
    }

    let _ = app.emit("snapshot-updated", &snapshot);
}

/// One poll tick as a fold over the previous Snapshot. Reads credentials, fetches
/// usage, and applies the failure policy (Decisions #2, #3, #9). `fail_count` is
/// mutated in place: reset on success, incremented on every failure.
fn poll_once(last: &Snapshot, fail_count: &mut u32, selected: Option<&str>) -> Snapshot {
    let credentials = match read_claude_credentials() {
        Ok(credentials) => credentials,
        Err(_) => {
            // No credentials at all → immediately Blind; nothing to be stale about.
            *fail_count += 1;
            return Snapshot {
                plan: last.plan.clone(),
                gauges: last.gauges.clone(),
                driving_idx: last.driving_idx,
                mood: None,
                status: Status::Blind,
                status_note: Some("no credentials".to_string()),
                retry_at: None,
                fetched_at: last.fetched_at,
            };
        }
    };

    match fetch_usage(&credentials.access_token) {
        Ok(gauges) => {
            *fail_count = 0;
            let driving_idx = driving_gauge(&gauges, selected);
            let mood = driving_idx.map(|idx| mood_for(gauges[idx].utilization));
            Snapshot {
                plan: plan_label(credentials.subscription_type.as_deref()),
                gauges,
                driving_idx,
                mood,
                status: Status::Normal,
                status_note: None,
                retry_at: None,
                fetched_at: Some(now_millis()),
            }
        }
        // AUTH failure (401): the token is dead. Keep showing the last reading as
        // Stale for a few polls (Claude Code may refresh it), then go Blind once we
        // hit the threshold. Only auth failures ever count toward Blind.
        Err(FetchError::Unauthorized) => {
            *fail_count += 1;
            let blind = *fail_count >= BLIND_AFTER_FAILURES;
            Snapshot {
                plan: last.plan.clone(),
                gauges: last.gauges.clone(),
                driving_idx: last.driving_idx,
                mood: if blind { None } else { last.mood.clone() },
                status: if blind { Status::Blind } else { Status::Stale },
                status_note: Some("session expired".to_string()),
                retry_at: None,
                fetched_at: last.fetched_at,
            }
        }
        // TRANSIENT failure (429 rate limit / offline / 5xx / unreadable): we had
        // valid data minutes ago — NEVER blind. Hold the last gauges and Mood as
        // Stale with a warning; the next successful poll clears it. Does NOT touch
        // `fail_count`, so a rate-limit can't spiral the Eye into Blind.
        // TRANSIENT failure (429 rate limit / offline / 5xx / unreadable): NEVER
        // blind. A 429 with a Retry-After sets `retry_at` so the Popover counts
        // down and the poll backs off to that time (below); everything else just
        // waits for the next normal poll.
        Err(error) => {
            let retry_at = match &error {
                FetchError::RateLimited(Some(secs)) => {
                    Some(now_millis() + (*secs as i64) * 1000)
                }
                _ => None,
            };
            Snapshot {
                plan: last.plan.clone(),
                gauges: last.gauges.clone(),
                driving_idx: last.driving_idx,
                mood: last.mood.clone(),
                status: Status::Stale,
                status_note: Some(transient_note(&error)),
                retry_at,
                fetched_at: last.fetched_at,
            }
        }
    }
}

/// The Stale-warning phrase for a transient fetch failure, shown in the Popover.
/// The rate-limit RETRY TIME is carried separately (`Snapshot.retry_at`) so the
/// Popover can render a live countdown — this is just the reason.
fn transient_note(error: &FetchError) -> String {
    match error {
        FetchError::RateLimited(_) => "rate limited".to_string(),
        FetchError::Http(code) => format!("server error ({code})"),
        FetchError::Network(_) => "offline".to_string(),
        FetchError::Parse(_) => "unreadable response".to_string(),
        // Unauthorized is handled on its own auth branch, never here.
        FetchError::Unauthorized => "session expired".to_string(),
    }
}

/// Map the Keychain `subscriptionType` to the Popover's Plan label.
fn plan_label(subscription_type: Option<&str>) -> Option<String> {
    subscription_type.map(|raw| match raw {
        "max" => "Max".to_string(),
        "pro" => "Pro".to_string(),
        "team" => "Team".to_string(),
        "enterprise" => "Enterprise".to_string(),
        other => title_case(other),
    })
}

/// Capitalize the first character; fallback for unknown plan strings.
fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Log the current Snapshot. NEVER logs the token — only status/mood/gauges.
fn log_snapshot(snapshot: &Snapshot) {
    println!(
        "[mad-eye] status={:?} mood={:?} plan={:?} driving_idx={:?} gauges={:?}",
        snapshot.status, snapshot.mood, snapshot.plan, snapshot.driving_idx, snapshot.gauges
    );
}

/// Current time as epoch millis (matches the TS `fetchedAt` contract).
fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}
