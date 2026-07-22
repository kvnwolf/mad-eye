//! Contract tests for the headless usage data core.
//!
//! These tests are the FIXED CONTRACT for the implementor. Every expected value
//! below is INDEPENDENT of how the code computes it — each is a literal stated by
//! the spec (Mood bands, the driving-gauge rule, the camelCase wire format) or
//! read straight off the REAL endpoint response captured from a live account on
//! 2026-07-22 (the `limits[]` sample). Do NOT edit them to make the code pass —
//! make the code satisfy them.
//!
//! They exercise the PUBLIC interface only. `usage` and its submodules are PUBLIC
//! on the lib crate so these integration tests can reach the pure logic:
//!   pub mod usage;                                 // in lib.rs
//!   pub mod types; pub mod mood; pub mod client;   // in usage/mod.rs
//! and `Gauge`, `GaugeKind`, `Mood`, `Status` derive `Debug + PartialEq` (plus
//! `Serialize`) so the assertions below compile.

use mad_eye_lib::usage::client::{parse_usage, FetchError};
use mad_eye_lib::usage::mood::{driving_gauge, mood_for};
use mad_eye_lib::usage::types::{Gauge, GaugeKind, Mood, Snapshot, Status};

/// Build a Gauge with only the fields a test cares about pinned; `name` and
/// `resets_at` are placeholders where the test asserts on kind/utilization only.
fn gauge(kind: GaugeKind, utilization: f64) -> Gauge {
    Gauge {
        name: "placeholder".to_string(),
        kind,
        utilization,
        resets_at: "2026-07-28T00:00:00.000000Z".to_string(),
    }
}

// ---------------------------------------------------------------------------
// mood_for — Mood is a PURE function of the driving %, in discrete bands.
// Expected values come straight from the spec bands:
//   <50 Calm · <80 Nervous · <95 Paranoid · <100 Frantic · >=100 Shattered
//   (clamp anything >100 to Shattered). (STATE.md Decision #3 / task spec.)
// ---------------------------------------------------------------------------
mod mood_bands {
    use super::*;

    #[test]
    fn calm_below_fifty() {
        // Spec: <50 => Calm. 0 and just under the 50 boundary are Calm.
        assert_eq!(mood_for(0.0), Mood::Calm);
        assert_eq!(mood_for(49.0), Mood::Calm);
        assert_eq!(mood_for(49.9), Mood::Calm);
    }

    #[test]
    fn nervous_from_fifty_to_eighty() {
        // Spec: 50 is NOT <50, so it falls into <80 => Nervous.
        assert_eq!(mood_for(50.0), Mood::Nervous);
        assert_eq!(mood_for(79.0), Mood::Nervous);
    }

    #[test]
    fn paranoid_from_eighty_to_ninetyfive() {
        // Spec: 80 => Paranoid, up to just under 95.
        assert_eq!(mood_for(80.0), Mood::Paranoid);
        assert_eq!(mood_for(94.0), Mood::Paranoid);
    }

    #[test]
    fn frantic_from_ninetyfive_to_hundred() {
        // Spec: 95 => Frantic, up to just under 100.
        assert_eq!(mood_for(95.0), Mood::Frantic);
        assert_eq!(mood_for(99.0), Mood::Frantic);
        assert_eq!(mood_for(99.9), Mood::Frantic);
    }

    #[test]
    fn shattered_at_hundred_and_clamped_above() {
        // Spec: >=100 => Shattered; values above 100 clamp to Shattered.
        assert_eq!(mood_for(100.0), Mood::Shattered);
        assert_eq!(mood_for(150.0), Mood::Shattered);
    }
}

// ---------------------------------------------------------------------------
// driving_gauge — picks the index that drives the Eye, SELECTION-aware.
//   selected name present        => that gauge's index (EVEN IF not the max).
//   no selection                 => the Session gauge index (the default).
//   no selection AND no Session  => the max-utilization gauge among all.
//   selected name NOT present    => falls back to the Session gauge.
//   empty                        => None (whatever the selection).
// (Revised spec: the default is the Session gauge; a user selection BY NAME
// overrides it and is persisted in lib.rs.) Expected indices are stated outright.
// ---------------------------------------------------------------------------
mod driving_gauge_selection {
    use super::*;

    /// A Gauge with a real `name` (the selection key) alongside kind/utilization.
    fn named(name: &str, kind: GaugeKind, utilization: f64) -> Gauge {
        Gauge {
            name: name.to_string(),
            kind,
            utilization,
            resets_at: "2026-07-28T00:00:00.000000Z".to_string(),
        }
    }

    #[test]
    fn selected_name_present_wins_even_when_not_the_max() {
        // "Weekly · Fable" (index 2) sits at 9 while Session (index 0) is the max
        // at 90; selecting Fable BY NAME must drive the Eye => index 2.
        let gauges = vec![
            named("Session", GaugeKind::Session, 90.0),
            named("Weekly · all models", GaugeKind::WeeklyAll, 40.0),
            named("Weekly · Fable", GaugeKind::WeeklyScoped, 9.0),
        ];
        assert_eq!(driving_gauge(&gauges, Some("Weekly · Fable")), Some(2));
    }

    #[test]
    fn no_selection_defaults_to_the_session_gauge() {
        // No selection => the Session gauge (index 1), even though WeeklyAll
        // (index 2) is the max at 88.
        let gauges = vec![
            gauge(GaugeKind::WeeklyScoped, 30.0),
            gauge(GaugeKind::Session, 20.0),
            gauge(GaugeKind::WeeklyAll, 88.0),
        ];
        assert_eq!(driving_gauge(&gauges, None), Some(1));
    }

    #[test]
    fn no_selection_and_no_session_falls_back_to_max() {
        // No selection and no Session gauge => the max-utilization gauge. WeeklyAll
        // at 88 (index 1) beats the scoped 40 (index 0).
        let gauges = vec![
            gauge(GaugeKind::WeeklyScoped, 40.0),
            gauge(GaugeKind::WeeklyAll, 88.0),
        ];
        assert_eq!(driving_gauge(&gauges, None), Some(1));
    }

    #[test]
    fn selected_name_absent_falls_back_to_session() {
        // The selected name is not among the gauges => fall back to the default
        // Session gauge (index 0), NOT the max (WeeklyAll at 95, index 1).
        let gauges = vec![
            gauge(GaugeKind::Session, 12.0),
            gauge(GaugeKind::WeeklyAll, 95.0),
        ];
        assert_eq!(driving_gauge(&gauges, Some("Weekly · Sonnet")), Some(0));
    }

    #[test]
    fn none_when_empty() {
        let gauges: Vec<Gauge> = Vec::new();
        assert_eq!(driving_gauge(&gauges, None), None);
        assert_eq!(driving_gauge(&gauges, Some("Session")), None);
    }
}

// ---------------------------------------------------------------------------
// parse_usage — maps the endpoint `limits[]` into Gauges. The JSON below is the
// REAL response captured from a live account on 2026-07-22: each entry's
// `percent` is ALREADY a 0–100 integer, so NO scale guard is applied. The
// field/kind/name mapping is a spec-stated literal.
// ---------------------------------------------------------------------------
mod parse_limits {
    use super::*;

    #[test]
    fn maps_the_three_real_limits_to_named_gauges() {
        // Straight from the real response: session/26, weekly_all/13,
        // weekly_scoped(Fable)/9. Kinds/names/order from the task mapping.
        let json = r#"{
            "limits": [
                { "kind": "session",       "group": "session", "percent": 26, "severity": "normal", "resets_at": "2026-07-22T09:29:59.572828+00:00", "scope": null, "is_active": true },
                { "kind": "weekly_all",    "group": "weekly",  "percent": 13, "severity": "normal", "resets_at": "2026-07-28T21:59:59.572851+00:00", "scope": null, "is_active": false },
                { "kind": "weekly_scoped", "group": "weekly",  "percent": 9,  "severity": "normal", "resets_at": "2026-07-28T21:59:59.573090+00:00", "scope": { "model": { "id": null, "display_name": "Fable" }, "surface": null }, "is_active": false }
            ]
        }"#;

        let expected = vec![
            Gauge {
                name: "Session".to_string(),
                kind: GaugeKind::Session,
                utilization: 26.0,
                resets_at: "2026-07-22T09:29:59.572828+00:00".to_string(),
            },
            Gauge {
                name: "Weekly · all models".to_string(),
                kind: GaugeKind::WeeklyAll,
                utilization: 13.0,
                resets_at: "2026-07-28T21:59:59.572851+00:00".to_string(),
            },
            Gauge {
                name: "Weekly · Fable".to_string(),
                kind: GaugeKind::WeeklyScoped,
                utilization: 9.0,
                resets_at: "2026-07-28T21:59:59.573090+00:00".to_string(),
            },
        ];

        assert_eq!(parse_usage(json).unwrap(), expected);
    }

    #[test]
    fn preserves_order_and_skips_unknown_kinds() {
        // An unknown `kind` (a future limit type) is skipped, not an error; the
        // surviving gauges keep their `limits[]` order (scoped, then session).
        let json = r#"{
            "limits": [
                { "kind": "weekly_scoped", "percent": 9,  "resets_at": "2026-07-28T21:59:59Z", "scope": { "model": { "display_name": "Fable" } } },
                { "kind": "future_kind",   "percent": 50, "resets_at": "2026-07-28T21:59:59Z", "scope": null },
                { "kind": "session",       "percent": 26, "resets_at": "2026-07-22T09:29:59Z", "scope": null }
            ]
        }"#;

        let gauges = parse_usage(json).unwrap();
        let kinds: Vec<&GaugeKind> = gauges.iter().map(|g| &g.kind).collect();
        assert_eq!(kinds, vec![&GaugeKind::WeeklyScoped, &GaugeKind::Session]);
    }

    #[test]
    fn scoped_name_uses_model_display_name_else_falls_back() {
        // weekly_scoped with a model => "Weekly · <display_name>";
        // weekly_scoped whose scope has no model name => "Weekly · model".
        let json = r#"{
            "limits": [
                { "kind": "weekly_scoped", "percent": 9, "resets_at": "2026-07-28T21:59:59Z", "scope": { "model": { "display_name": "Fable" } } },
                { "kind": "weekly_scoped", "percent": 5, "resets_at": "2026-07-28T21:59:59Z", "scope": null }
            ]
        }"#;

        let gauges = parse_usage(json).unwrap();
        let names: Vec<&str> = gauges.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, vec!["Weekly · Fable", "Weekly · model"]);
        assert!(gauges.iter().all(|g| g.kind == GaugeKind::WeeklyScoped));
    }

    #[test]
    fn percent_is_used_verbatim_no_scale_guard() {
        // `percent` is authoritative and already 0–100: a small integer like 9
        // stays 9.0 — it is NOT multiplied by 100 the way legacy 0..1 windows are.
        let json = r#"{
            "limits": [
                { "kind": "session", "percent": 9, "resets_at": "2026-07-22T09:29:59Z", "scope": null }
            ]
        }"#;

        let utils: Vec<f64> = parse_usage(json).unwrap().iter().map(|g| g.utilization).collect();
        assert_eq!(utils, vec![9.0]);
    }

    #[test]
    fn empty_limits_with_no_legacy_windows_is_empty() {
        // No limits and no legacy windows => empty Vec (the Popover renders this
        // as "no data"), NOT an error.
        let json = r#"{ "limits": [] }"#;
        assert_eq!(parse_usage(json).unwrap(), Vec::new());
    }

    #[test]
    fn absent_limits_falls_back_to_legacy_windows() {
        // Older-shape account: no `limits[]`, data in the top-level windows. The
        // legacy path still maps fields->kinds and applies the ×100 guard to a
        // 0..1 fraction (0.5 => 50.0); a percent >1 (13.0/9.0) passes through.
        // opus scopes to "Weekly · Fable" as WeeklyScoped.
        let json = r#"{
            "five_hour":      { "utilization": 0.5,  "resets_at": "2026-07-22T09:29:59Z" },
            "seven_day":      { "utilization": 13.0, "resets_at": "2026-07-28T21:59:59Z" },
            "seven_day_opus": { "utilization": 9.0,  "resets_at": "2026-07-28T21:59:59Z" }
        }"#;

        let gauges = parse_usage(json).unwrap();
        let summary: Vec<(&str, &GaugeKind, f64)> = gauges
            .iter()
            .map(|g| (g.name.as_str(), &g.kind, g.utilization))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("Session", &GaugeKind::Session, 50.0),
                ("Weekly · all models", &GaugeKind::WeeklyAll, 13.0),
                ("Weekly · Fable", &GaugeKind::WeeklyScoped, 9.0),
            ]
        );
    }

    #[test]
    fn malformed_json_is_a_parse_error() {
        // Garbage input must surface as FetchError::Parse, not a panic.
        let result = parse_usage("this is not json {");
        assert!(matches!(result, Err(FetchError::Parse(_))));
    }
}

// ---------------------------------------------------------------------------
// Snapshot / Gauge / GaugeKind / Status serialization — the seam to the TS
// Popover (Decision #8). The wire format is camelCase, and Mood/GaugeKind/Status
// serialize as their camelCase/lowercase string names. These key/string literals
// ARE the contract the TS side reads; if the implementor forgets
// `#[serde(rename_all = ...)]` serde defaults to snake_case and these fail.
// ---------------------------------------------------------------------------
mod snapshot_seam {
    use super::*;

    #[test]
    fn snapshot_serializes_with_camelcase_keys() {
        let snap = Snapshot {
            plan: Some("Max".to_string()),
            gauges: Vec::new(),
            driving_idx: Some(2),
            mood: None,
            status: Status::Normal,
            status_note: None,
            retry_at: None,
            fetched_at: Some(1_752_000_000),
        };

        let value = serde_json::to_value(&snap).unwrap();
        let obj = value.as_object().expect("Snapshot serializes to a JSON object");

        // camelCase keys the Popover expects — present.
        assert!(obj.contains_key("drivingIdx"), "expected key `drivingIdx`");
        assert!(obj.contains_key("statusNote"), "expected key `statusNote`");
        assert!(obj.contains_key("retryAt"), "expected key `retryAt`");
        assert!(obj.contains_key("fetchedAt"), "expected key `fetchedAt`");
        assert!(obj.contains_key("plan"));
        assert!(obj.contains_key("gauges"));
        assert!(obj.contains_key("mood"));
        assert!(obj.contains_key("status"));

        // snake_case Rust field names must NOT leak onto the wire.
        assert!(!obj.contains_key("driving_idx"), "`driving_idx` leaked (rename missing)");
        assert!(!obj.contains_key("status_note"), "`status_note` leaked (rename missing)");
        assert!(!obj.contains_key("retry_at"), "`retry_at` leaked (rename missing)");
        assert!(!obj.contains_key("fetched_at"), "`fetched_at` leaked (rename missing)");

        // Values survive intact (numeric accessors dodge Number-variant quirks).
        assert_eq!(value["drivingIdx"].as_u64(), Some(2));
        assert_eq!(value["plan"].as_str(), Some("Max"));
        assert_eq!(value["fetchedAt"].as_i64(), Some(1_752_000_000));
    }

    #[test]
    fn gauge_serializes_with_camelcase_keys() {
        let g = Gauge {
            name: "Session".to_string(),
            kind: GaugeKind::Session,
            utilization: 33.0,
            resets_at: "2026-07-21T18:00:00.000000Z".to_string(),
        };

        let value = serde_json::to_value(&g).unwrap();
        let obj = value.as_object().expect("Gauge serializes to a JSON object");

        assert!(obj.contains_key("resetsAt"), "expected key `resetsAt`");
        assert!(!obj.contains_key("resets_at"), "`resets_at` leaked (rename missing)");
        assert_eq!(value["name"].as_str(), Some("Session"));
        assert_eq!(value["utilization"].as_f64(), Some(33.0));
        assert_eq!(value["resetsAt"].as_str(), Some("2026-07-21T18:00:00.000000Z"));
    }

    #[test]
    fn gauge_kind_serializes_camelcase() {
        // The wire values mirror the TS GaugeKind union
        // ('session' | 'weeklyAll' | 'weeklyScoped').
        assert_eq!(serde_json::to_value(GaugeKind::Session).unwrap(), serde_json::json!("session"));
        assert_eq!(serde_json::to_value(GaugeKind::WeeklyAll).unwrap(), serde_json::json!("weeklyAll"));
        assert_eq!(
            serde_json::to_value(GaugeKind::WeeklyScoped).unwrap(),
            serde_json::json!("weeklyScoped")
        );
    }

    #[test]
    fn status_serializes_lowercase() {
        // Spec: Status serializes lowercase via serde rename_all.
        assert_eq!(serde_json::to_value(Status::Normal).unwrap(), serde_json::json!("normal"));
        assert_eq!(serde_json::to_value(Status::Stale).unwrap(), serde_json::json!("stale"));
        assert_eq!(serde_json::to_value(Status::Blind).unwrap(), serde_json::json!("blind"));
        assert_eq!(serde_json::to_value(Status::Asleep).unwrap(), serde_json::json!("asleep"));
    }
}
