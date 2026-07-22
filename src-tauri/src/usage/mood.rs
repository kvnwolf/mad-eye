//! Pure, IO-free logic: Mood bands and Driving-Gauge selection.
//!
//! Both functions are the tested contract (`tests/usage_core.rs`). They must
//! stay pure so the tests never touch the network or the Keychain.

use std::cmp::Ordering;

use super::types::{Gauge, GaugeKind, Mood};

/// Map a Driving Gauge percentage to a discrete Mood (Decision #3).
///
/// Bands: `<50` Calm · `<80` Nervous · `<95` Paranoid · `<100` Frantic ·
/// `>=100` Shattered. Anything above 100 clamps to Shattered.
pub fn mood_for(pct: f64) -> Mood {
    if pct < 50.0 {
        Mood::Calm
    } else if pct < 80.0 {
        Mood::Nervous
    } else if pct < 95.0 {
        Mood::Paranoid
    } else if pct < 100.0 {
        Mood::Frantic
    } else {
        Mood::Shattered
    }
}

/// Pick the index of the Gauge that drives the Eye (Decision #2, revised).
///
/// A user SELECTION wins: if `selected` names a Gauge that is present, that Gauge
/// drives the Eye — even when it is not the max. Otherwise the default is the
/// **Session** Gauge (`GaugeKind::Session`); with no Session Gauge, fall back to
/// the max-utilization Gauge among all. `None` for an empty slice. (This replaces
/// the old WeeklyScoped-first rule — the per-model default is gone; it is now
/// Session-default plus click-to-select persisted in `lib.rs`.)
pub fn driving_gauge(gauges: &[Gauge], selected: Option<&str>) -> Option<usize> {
    // A present user selection always wins.
    if let Some(name) = selected {
        if let Some(idx) = gauges.iter().position(|gauge| gauge.name == name) {
            return Some(idx);
        }
    }

    // Default: the Session Gauge.
    if let Some(idx) = gauges
        .iter()
        .position(|gauge| gauge.kind == GaugeKind::Session)
    {
        return Some(idx);
    }

    // No Session Gauge: fall back to the max-utilization Gauge among all.
    gauges
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            a.utilization
                .partial_cmp(&b.utilization)
                .unwrap_or(Ordering::Equal)
        })
        .map(|(idx, _)| idx)
}
