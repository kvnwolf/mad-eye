//! Contract test for the Eye animator's ONE pure function (Task 3).
//!
//! Written from the spec ALONE (STATE.md ## Spec + the task's named symbols),
//! before any implementation existed. This is the FIXED CONTRACT: do NOT edit
//! it to make it pass — make `ease_in_out_sine` satisfy it.
//!
//! Scope. The task names `ease_in_out_sine` as "the ONE tested pure fn". The
//! rest of the animator (the 13 `POSITIONS`, `EyeState`, `Animator::spawn`) is
//! deliberately NOT unit-tested here: `spawn` starts a `std::thread`, reaches
//! for the live tray via `app.tray_by_id("eye")`, and calls
//! `set_icon_with_as_template` — un-mockable Tauri boundaries the interface
//! constructs internally. Per the spec those paths are verified MANUALLY
//! (`MAD_EYE_FAKE_PCT=<0..100>` + `bun tauri dev`), not in `cargo test`.
//!
//! Independence of expected values (the anti-tautology rule). None of the
//! numbers below are produced by running the code's own expression
//! `0.5 - 0.5 * (PI * t).cos()`. They are either:
//!   * literals the SPEC states outright (0->0, 0.5->0.5, 1->1; clamp to [0,1]),
//!   * or derived a DIFFERENT way — from the trig identity cos(45 deg) = √2/2,
//!     giving the closed forms (2-√2)/4 and (2+√2)/4 for t = 0.25 / 0.75 — so a
//!     wrong easing (linear, ease-quad, a sin/cos swap, a dropped clamp, a
//!     flipped curve) breaks the assertion instead of silently tracking the bug.
//!
//! Interface requirement (see findings). For this integration test to reach the
//! function, the module path must be PUBLIC on the lib crate:
//!   pub mod eye;              // in lib.rs (currently `mod eye;`)
//!   pub mod animator;         // in eye/mod.rs
//!   pub fn ease_in_out_sine(t: f64) -> f64;   // in eye/animator.rs

use mad_eye_lib::eye::animator::ease_in_out_sine;

/// Absolute-error tolerance: far larger than f64 noise (~1e-16) yet far smaller
/// than the gap any wrong easing would open (>1e-2), so it only ever forgives
/// floating-point rounding, never a real defect.
const EPS: f64 = 1e-9;

fn approx(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < EPS
}

// ---------------------------------------------------------------------------
// Boundaries — the spec names these outputs directly: "0->0, .5->.5, 1->1".
// At t=0 the curve starts flat at 0, at t=1 it ends at 1, and it crosses its
// own midpoint at t=0.5. These are the load-bearing anchors of the ease.
// ---------------------------------------------------------------------------

#[test]
fn starts_at_zero() {
    // Spec: ease_in_out_sine(0) == 0.
    assert!(approx(ease_in_out_sine(0.0), 0.0), "f(0) should be 0");
}

#[test]
fn crosses_its_midpoint_at_half() {
    // Spec: ease_in_out_sine(0.5) == 0.5 (the S-curve's centre of symmetry).
    assert!(approx(ease_in_out_sine(0.5), 0.5), "f(0.5) should be 0.5");
}

#[test]
fn ends_at_one() {
    // Spec: ease_in_out_sine(1) == 1.
    assert!(approx(ease_in_out_sine(1.0), 1.0), "f(1) should be 1");
}

// ---------------------------------------------------------------------------
// Shape — pins that this is the SINE ease, not merely SOME monotonic 0->1 map.
// A linear ramp would give 0.25 here; ease-in-quad would give 0.0625; only the
// sine ease gives (2-√2)/4. Expected literals come from cos(45 deg)=√2/2, a
// derivation independent of the code's `0.5 - 0.5*cos(...)` expression.
// ---------------------------------------------------------------------------

#[test]
fn quarter_point_matches_the_sine_ease() {
    // (2 - √2) / 4 = (2 - 1.4142135623730951) / 4 = 0.14644660940672623.
    assert!(
        approx(ease_in_out_sine(0.25), 0.14644660940672623),
        "f(0.25) should be (2-√2)/4 ≈ 0.1464466",
    );
}

#[test]
fn three_quarter_point_matches_the_sine_ease() {
    // (2 + √2) / 4 = (2 + 1.4142135623730951) / 4 = 0.8535533905932738.
    assert!(
        approx(ease_in_out_sine(0.75), 0.8535533905932738),
        "f(0.75) should be (2+√2)/4 ≈ 0.8535534",
    );
}

// ---------------------------------------------------------------------------
// Point symmetry — an independent PROPERTY of the ease-in-out sine: it is
// symmetric about (0.5, 0.5), so f(t) + f(1-t) == 1 for every t. This holds by
// the identity cos(π-x) = -cos(x), reasoned WITHOUT recomputing the formula,
// and it catches sin/cos swaps and asymmetric substitutes a linear ramp would
// still pass. (Also demonstrates it never leaves [0,1] over the sampled range.)
// ---------------------------------------------------------------------------

#[test]
fn is_point_symmetric_about_the_centre() {
    for &t in &[0.1, 0.2, 0.25, 0.4, 0.6, 0.75, 0.8, 0.9] {
        let sum = ease_in_out_sine(t) + ease_in_out_sine(1.0 - t);
        assert!(approx(sum, 1.0), "f({t}) + f(1-{t}) should be 1, got {sum}");
    }
}

#[test]
fn stays_within_the_unit_interval() {
    // The pupil scaling relies on the ease never exceeding [0,1]; sample densely.
    for i in 0..=100 {
        let y = ease_in_out_sine(i as f64 / 100.0);
        assert!((0.0..=1.0).contains(&y), "f({}) = {y} escaped [0,1]", i as f64 / 100.0);
    }
}

// ---------------------------------------------------------------------------
// Monotonic — the spec asks for "monotonic". A dart must sweep the pupil in one
// direction with no backtracking, so the ease rises across the whole interval.
// The RELATION (each output greater than the last) is the spec's, not a value
// recomputed from the code.
// ---------------------------------------------------------------------------

#[test]
fn is_strictly_increasing_across_the_interval() {
    let mut prev = ease_in_out_sine(0.0);
    for i in 1..=50 {
        let t = i as f64 / 50.0;
        let y = ease_in_out_sine(t);
        assert!(y > prev, "f({t}) = {y} should exceed the previous sample {prev}");
        prev = y;
    }
}

// ---------------------------------------------------------------------------
// Clamping — spec: "t clamped to [0,1]". Inputs below 0 behave as t=0 (=> 0) and
// inputs above 1 behave as t=1 (=> 1). Without the clamp the raw cosine is even
// and periodic, so f(-0.5) would read 0.5 and f(2.0) would read 0.0 — this test
// is exactly what fails if the clamp is dropped.
// ---------------------------------------------------------------------------

#[test]
fn clamps_inputs_below_zero_to_the_start() {
    assert!(approx(ease_in_out_sine(-0.5), 0.0), "f(-0.5) should clamp to f(0)=0");
    assert!(approx(ease_in_out_sine(-1000.0), 0.0), "large negative should clamp to 0");
}

#[test]
fn clamps_inputs_above_one_to_the_end() {
    assert!(approx(ease_in_out_sine(2.0), 1.0), "f(2.0) should clamp to f(1)=1");
    assert!(approx(ease_in_out_sine(1000.0), 1.0), "large positive should clamp to 1");
}
