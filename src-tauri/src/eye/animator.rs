//! The Eye's motion: a background thread that darts the pupil by Mood.
//!
//! The tray animates by swapping one tiny-skia frame at a time (~30fps). Each
//! "dart" is a burst of frames sweeping the pupil from its previous position to
//! a new one along an ease-in-out-sine curve; between darts the thread sleeps a
//! per-Mood tick interval. All motion numbers are the locked prototype verdict
//! (variant D) recorded in STATE.md.
//!
//! Everything here except [`ease_in_out_sine`] is un-mockable (it starts a
//! `std::thread`, reaches the live tray via `app.tray_by_id("eye")`, and calls
//! `set_icon_with_as_template`), so it is verified MANUALLY with
//! `MAD_EYE_FAKE_PCT=<0..100>` + `bun tauri dev`. `ease_in_out_sine` is the ONE
//! tested pure function (`tests/eye_animation.rs`).

use std::env;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};

use crate::eye::render::{render_frame, EyeFrame};
use crate::usage::mood::mood_for;
use crate::usage::types::{Mood, Status};
use crate::AppState;

// --- Motion constants (the locked prototype verdict — tunable here) ----------

/// Dart TRANSITION durations per Mood: how long the pupil takes to reach its
/// new position. Calmer moods glide; frantic snaps.
const CALM_DART_MS: u64 = 480;
const NERVOUS_DART_MS: u64 = 300;
const PARANOID_DART_MS: u64 = 170;
const FRANTIC_DART_MS: u64 = 100;

/// Tick INTERVALS between darts per Mood: how long the pupil holds still before
/// the next glance. Calm is an idle: one slow, wide glance every ~9.5s.
const CALM_TICK_MS: u64 = 9500;
const NERVOUS_TICK_MS: u64 = 650;
const PARANOID_TICK_MS: u64 = 220;
const FRANTIC_TICK_MS: u64 = 60;

/// Per-Mood amplitude as a multiple of [`MAX_TRAVEL`]. These already fold in the
/// Amplitud ×1.5 knob (base 0.4/0.5/0.8/1.0 → 0.6/0.75/1.2/1.5). The clamp to
/// the ring guards the paranoid/frantic values that exceed 1.0.
const CALM_AMPLITUDE: f64 = 0.6;
const NERVOUS_AMPLITUDE: f64 = 0.75;
const PARANOID_AMPLITUDE: f64 = 1.2;
const FRANTIC_AMPLITUDE: f64 = 1.5;

/// Frame cadence — ~30fps. Frames per dart ≈ `duration_ms / FRAME_MS`.
const FRAME_MS: f64 = 33.0;

/// How often the static states (Asleep / Blind / Shattered) re-render the same
/// frozen frame and re-check the desired state — keeps waking responsive.
const IDLE_TICK_MS: u64 = 500;

/// Pupil travel (in render.rs viewBox units) at amplitude 1.0. A magnitude-1
/// position scaled by amplitude `a` lands `a * MAX_TRAVEL` units off centre.
/// Larger than the pupil radius so the dart is clearly visible at 18px.
const MAX_TRAVEL: f64 = 14.0;

/// Geometric safe radius (viewBox units): the pupil CENTRE must stay within the
/// ring's inner edge (ring r43 stroke 10 → inner ≈ 38, pupil r13 → ≈ 25).
const SAFE_RADIUS: f64 = 25.0;

/// The pupil centre is clamped to 0.9× the safe radius, leaving easing-overshoot
/// headroom so a dart never nudges the pupil outside the ring.
const CLAMP_RADIUS: f64 = SAFE_RADIUS * 0.9;

/// The 13 pupil positions the Eye darts between, as normalized `(x, y)` in
/// `[-1, 1]`: centre, 4 cardinals + 4 diagonals at magnitude 1, and 4
/// half-distance cardinals at magnitude 0.5. Scaled by amplitude × MAX_TRAVEL
/// (then ring-clamped) to become an [`EyeFrame::pupil`] viewBox offset.
const DIAG: f64 = std::f64::consts::FRAC_1_SQRT_2; // √2/2 ≈ 0.7071 → magnitude 1

const POSITIONS: [(f64, f64); 13] = [
    (0.0, 0.0), // centre
    // 4 cardinals (magnitude 1)
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    // 4 diagonals (magnitude 1)
    (DIAG, DIAG),
    (-DIAG, DIAG),
    (DIAG, -DIAG),
    (-DIAG, -DIAG),
    // 4 half-distance cardinals (magnitude 0.5)
    (0.5, 0.0),
    (-0.5, 0.0),
    (0.0, 0.5),
    (0.0, -0.5),
];

/// The ease-in-out-sine curve the darts sweep along (the ONE tested pure fn).
///
/// `t` is clamped to `[0, 1]`; `f(0) = 0`, `f(0.5) = 0.5`, `f(1) = 1`, and the
/// curve is monotonic and point-symmetric about its midpoint.
pub fn ease_in_out_sine(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    0.5 - 0.5 * (std::f64::consts::PI * t).cos()
}

/// What the animator should render this cycle. Derived either from the real
/// [`AppState`] snapshot or, when `MAD_EYE_FAKE_PCT` is set, from that fake %.
#[derive(Debug, Clone, PartialEq)]
enum EyeState {
    /// Startup / no data yet: lid closed, no darting.
    Asleep,
    /// Fetch failed past the threshold: lid closed, no darting.
    Blind,
    /// A moving Mood (calm/nervous/paranoid/frantic): the pupil darts.
    Mood(Mood),
    /// Driving Gauge at 100%: frozen wide, pupil centred, cracks.
    Shattered,
}

/// Owns the animation thread so it lives for the app's lifetime.
pub struct Animator {
    #[allow(dead_code)]
    handle: thread::JoinHandle<()>,
}

impl Animator {
    /// Start the animation thread. It reads the desired [`EyeState`] each cycle
    /// and drives the tray via atomic template-icon swaps.
    pub fn spawn(app: AppHandle) -> Animator {
        let handle = thread::spawn(move || run(app));
        Animator { handle }
    }
}

/// The animation loop: pick the desired state, render it, repeat forever.
fn run(app: AppHandle) {
    // A tiny std-only PRNG (xorshift32), seeded at launch so darts differ each
    // run. No external rng crate (ponytail); a mutable seed is all we need.
    let mut seed = seed_from_time();

    // The pupil's current viewBox offset and which POSITIONS index it came from,
    // so the next dart starts where the last one ended and picks a new target.
    let mut prev_pupil = (0.0, 0.0);
    let mut current_idx = 0usize;

    loop {
        match desired_state(&app) {
            EyeState::Asleep | EyeState::Blind => {
                // Closed lid, rendered once per idle tick; reset the pupil so
                // the next wake darts out from centre.
                render(&app, &closed_lid());
                prev_pupil = (0.0, 0.0);
                current_idx = 0;
                thread::sleep(Duration::from_millis(IDLE_TICK_MS));
            }
            EyeState::Shattered => {
                render(&app, &shattered_frame());
                prev_pupil = (0.0, 0.0);
                current_idx = 0;
                thread::sleep(Duration::from_millis(IDLE_TICK_MS));
            }
            EyeState::Mood(mood) => {
                // Every Mood — calm included — is just a glance to a new
                // position. Mad-Eye's enchanted eye is a socket prosthesis with
                // no lid, so it never blinks; calm is simply a slow, wide glance.
                let target_idx = pick_different(&mut seed, current_idx);
                let target = scaled_pupil(POSITIONS[target_idx], amplitude(&mood));
                dart(&app, prev_pupil, target, dart_duration(&mood));
                prev_pupil = target;
                current_idx = target_idx;

                thread::sleep(tick_interval(&mood));
            }
        }
    }
}

/// Read the desired [`EyeState`]. The `MAD_EYE_FAKE_PCT` dev override wins: any
/// parseable f64 drives the state from `mood_for(pct)` (Shattered at ≥100),
/// IGNORING the real snapshot's Asleep/Blind so animation is verifiable without
/// a live token. Otherwise: Asleep→Asleep, Blind→Blind, else the snapshot Mood.
fn desired_state(app: &AppHandle) -> EyeState {
    if let Ok(raw) = env::var("MAD_EYE_FAKE_PCT") {
        if let Ok(pct) = raw.trim().parse::<f64>() {
            return eye_state_from_mood(mood_for(pct));
        }
    }

    let Some(state) = app.try_state::<AppState>() else {
        // AppState not managed yet (very early startup): treat as asleep.
        return EyeState::Asleep;
    };
    let (status, mood) = {
        let snapshot = state.snapshot.lock().unwrap();
        (snapshot.status.clone(), snapshot.mood.clone())
    };

    match status {
        Status::Asleep => EyeState::Asleep,
        Status::Blind => EyeState::Blind,
        // Normal / Stale: animate the current Mood if we have one.
        _ => match mood {
            Some(mood) => eye_state_from_mood(mood),
            None => EyeState::Asleep,
        },
    }
}

/// Shattered is its own frozen state; every other Mood is a moving one.
fn eye_state_from_mood(mood: Mood) -> EyeState {
    match mood {
        Mood::Shattered => EyeState::Shattered,
        other => EyeState::Mood(other),
    }
}

/// Run one dart: `frames ≈ duration / 33ms` frames stepping `t` 0→1 through the
/// ease, interpolating the pupil from `from` to `to`. Each frame is an atomic
/// template-icon swap.
fn dart(app: &AppHandle, from: (f64, f64), to: (f64, f64), duration: Duration) {
    let frames = ((duration.as_millis() as f64 / FRAME_MS).round() as u64).max(1);
    for i in 1..=frames {
        let eased = ease_in_out_sine(i as f64 / frames as f64);
        let pupil = (
            from.0 + (to.0 - from.0) * eased,
            from.1 + (to.1 - from.1) * eased,
        );
        render(app, &open_frame(pupil));
        thread::sleep(Duration::from_millis(FRAME_MS as u64));
    }
}

/// Turn a normalized position into a pupil viewBox offset, ring-clamped so it
/// can never escape the ring even under easing overshoot.
fn scaled_pupil(pos: (f64, f64), amplitude: f64) -> (f64, f64) {
    clamp_to_ring((pos.0 * amplitude * MAX_TRAVEL, pos.1 * amplitude * MAX_TRAVEL))
}

/// Clamp a pupil offset so its magnitude never exceeds [`CLAMP_RADIUS`].
fn clamp_to_ring(p: (f64, f64)) -> (f64, f64) {
    let magnitude = (p.0 * p.0 + p.1 * p.1).sqrt();
    if magnitude > CLAMP_RADIUS {
        let scale = CLAMP_RADIUS / magnitude;
        (p.0 * scale, p.1 * scale)
    } else {
        p
    }
}

/// Pick a random position index that differs from `current` (uniform over the
/// other 12), so a dart always moves.
fn pick_different(seed: &mut u32, current: usize) -> usize {
    let candidate = next_rand(seed) as usize % (POSITIONS.len() - 1);
    if candidate >= current {
        candidate + 1
    } else {
        candidate
    }
}

/// xorshift32: a tiny std-only PRNG. `*seed` must be non-zero.
fn next_rand(seed: &mut u32) -> u32 {
    let mut x = *seed;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *seed = x;
    x
}

/// A non-zero xorshift seed derived from the launch instant.
fn seed_from_time() -> u32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(1);
    nanos | 1 // xorshift needs a non-zero seed
}

// --- Per-Mood params (the Shattered arms are unreachable: a Shattered Mood is
// mapped to EyeState::Shattered and never reaches a moving branch). ------------

fn dart_duration(mood: &Mood) -> Duration {
    Duration::from_millis(match mood {
        Mood::Calm => CALM_DART_MS,
        Mood::Nervous => NERVOUS_DART_MS,
        Mood::Paranoid => PARANOID_DART_MS,
        Mood::Frantic | Mood::Shattered => FRANTIC_DART_MS,
    })
}

fn tick_interval(mood: &Mood) -> Duration {
    Duration::from_millis(match mood {
        Mood::Calm => CALM_TICK_MS,
        Mood::Nervous => NERVOUS_TICK_MS,
        Mood::Paranoid => PARANOID_TICK_MS,
        Mood::Frantic | Mood::Shattered => FRANTIC_TICK_MS,
    })
}

fn amplitude(mood: &Mood) -> f64 {
    match mood {
        Mood::Calm => CALM_AMPLITUDE,
        Mood::Nervous => NERVOUS_AMPLITUDE,
        Mood::Paranoid => PARANOID_AMPLITUDE,
        Mood::Frantic | Mood::Shattered => FRANTIC_AMPLITUDE,
    }
}

// --- Frame builders ----------------------------------------------------------

fn open_frame(pupil: (f64, f64)) -> EyeFrame {
    EyeFrame {
        pupil,
        lid_closed: false,
        shattered: false,
    }
}

fn closed_lid() -> EyeFrame {
    EyeFrame {
        pupil: (0.0, 0.0),
        lid_closed: true,
        shattered: false,
    }
}

fn shattered_frame() -> EyeFrame {
    EyeFrame {
        pupil: (0.0, 0.0),
        lid_closed: false,
        shattered: true,
    }
}

/// Render one pose to the tray with the ATOMIC template swap (never plain
/// `set_icon`, which resets the template flag → flicker/wrong tint). Best-effort:
/// a transient tray error (e.g. mid-teardown) is ignored, not fatal.
fn render(app: &AppHandle, frame: &EyeFrame) {
    if let Some(tray) = app.tray_by_id("eye") {
        let _ = tray.set_icon_with_as_template(Some(render_frame(frame)), true);
    }
}
