# eye

How the Eye looks and moves: renders each Mood pose into a macOS template tray icon, and animates the pupil by Mood over time.

## Files

- `render.rs` — `render_frame(&EyeFrame) -> Image`: draws the Eye (bold ring + a stroked iris around a filled pupil, optional cracks / closed lid) with tiny-skia and returns an opaque-black-on-transparent image.
- `animator.rs` — `Animator::spawn(app)`: a background thread that reads the desired `EyeState` each cycle and drives the tray with atomic template-icon swaps (~30fps dart bursts). Holds the `ease_in_out_sine` curve, the 13 pupil `POSITIONS`, and the per-Mood motion constants.
- `mod.rs` — module declaration only (no barrel; import `render` / `animator` by deep path).

## Interface

- `EyeFrame { pupil: (f64, f64), lid_closed: bool, shattered: bool }` — one pose. `pupil` is an offset in viewBox (0..100) units from centre (50, 50); `(0, 0)` is centred. `EyeFrame::static_open()` is the calm, open, centred pose.
- `render_frame(frame) -> tauri::image::Image<'static>` — pure; renders `frame` to a 36×36 template image.
- `ease_in_out_sine(t) -> f64` — pure; the dart easing curve, `t` clamped to `[0, 1]` (`0→0`, `0.5→0.5`, `1→1`, monotonic, point-symmetric). The ONE tested pure fn (`tests/eye_animation.rs`).
- `Animator::spawn(app: AppHandle) -> Animator` — starts the animation thread and returns a handle to keep alive (managed by `lib.rs`). The thread reads its state from `AppState.snapshot` (via `app`), or from `MAD_EYE_FAKE_PCT` when that env var holds a parseable f64 (dev override — drives the Eye from `mood_for(pct)`, Shattered at ≥100, ignoring the real Asleep/Blind).

## Invariants

- Output is 36×36 physical px (18pt logical @2x retina) — the tray downscales to look crisp.
- Pure OPAQUE BLACK on transparent only (macOS template requirement); the tray must set `icon_as_template(true)` for light/dark tinting.
- Geometry is authored in a 0..100 viewBox and scaled ×0.36, then enlarged during the wrap smoke test to read bigger in the real menubar: ring r43 / stroke 10, iris r13 stroke-width 3.5 + pupil r6 (the moving element, variant B — replacing the old solid pupil r13; the iris keeps r13 as its extent so the animator's clamp is unchanged), closed lid x18→x82 @ y50 stroke 10 round caps, cracks stroke 6 round joins.
- tiny-skia `data()` is premultiplied RGBA8, but for pure black premultiplied == straight, so it is handed straight to `Image::new_owned` with NO conversion. This holds ONLY while the paint is pure black.
- Animation ALWAYS uses `set_icon_with_as_template(Some(frame), true)` — plain `set_icon` resets the template flag → flicker + wrong tint.
- Motion numbers come from the prototype verdict, punched up in the wrap smoke test so high Moods read as visibly more frantic in the menubar: per-Mood dart durations 480/300/170/100ms, ticks 9500/650/220/60ms, amplitudes 0.6/0.75/1.2/1.5, `MAX_TRAVEL` 14 (larger than the pupil radius so the dart is visible at 18px), ~30fps. The pupil is ALWAYS clamped to 0.9× the ring's safe radius so it never leaves the ring.
- **The Eye never blinks.** Mad-Eye's magical eye is a lidless socket prosthesis, so calm is just a slow, wide glance — no lid animation. The closed-lid frame is used ONLY for the Asleep and Blind states (eye off), never as a transient blink.
- The animator's tray access is un-mockable (thread + `tray_by_id` + `set_icon_with_as_template`), so only `ease_in_out_sine` is unit-tested; the rest is verified manually via `MAD_EYE_FAKE_PCT` + `bun tauri dev`.

## What's intentionally NOT here

- No Mood → pose mapping in `render.rs` (the animator picks pupil positions and flags; `render.rs` stays a pure draw).
- No SVG or PNG assets, and no `resvg` — the Eye is drawn directly.
- No external rng or async crate — randomness is a tiny std-only xorshift; scheduling is `std::thread` + `sleep`.
- The animator does not fetch or compute Mood — it only reads the already-computed `AppState.snapshot` (or the `MAD_EYE_FAKE_PCT` override).
