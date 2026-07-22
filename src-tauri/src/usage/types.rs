//! The wire types the whole app agrees on.
//!
//! Everything the Popover renders crosses this seam as a serialized `Snapshot`
//! (Decision #8). The serde attributes here ARE the contract with the TS side
//! (`src/popover/snapshot.ts`): `Snapshot`/`Gauge` keys are camelCase; `Mood`,
//! `GaugeKind`, and `Status` serialize as their lowercase/camelCase string names.

use serde::Serialize;

/// How agitated the Eye is — a pure function of the Driving Gauge % (bands in
/// `mood.rs`). Serializes lowercase (`"calm"`, `"nervous"`, …) for the Popover.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mood {
    Calm,
    Nervous,
    Paranoid,
    Frantic,
    Shattered,
}

/// Which usage limit a Gauge measures — mirrors the API's `limits[].kind`.
/// Serializes camelCase (`"session"`, `"weeklyAll"`, `"weeklyScoped"`) to mirror
/// the TS `GaugeKind`. `WeeklyScoped` is a per-model weekly window (e.g. Fable);
/// the model name is baked into `Gauge.name` (e.g. "Weekly · Fable").
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GaugeKind {
    Session,
    WeeklyAll,
    WeeklyScoped,
}

/// One usage-limit reading: percent used (0–100, post scale-guard) plus when its
/// window resets. `resets_at` is the raw ISO-8601 instant from the endpoint.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gauge {
    pub name: String,
    pub kind: GaugeKind,
    pub utilization: f64,
    pub resets_at: String,
}

/// Health of the reading behind a `Snapshot`. Serializes lowercase; the human
/// reason for a non-Normal status lives in `Snapshot.status_note`, not here.
/// `Stale` (transient failure — 429 / offline / 5xx) keeps the last gauges and
/// Mood visible with a warning; only auth failures (no creds / 401) reach `Blind`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Normal,
    Stale,
    Blind,
    Asleep,
}

/// Everything the Popover needs to draw one frame. Serialized camelCase for the
/// TS seam: `plan`, `gauges`, `drivingIdx`, `mood`, `status`, `statusNote`,
/// `retryAt`, `fetchedAt`. Holds NO secret — never the OAuth token.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Plan label (e.g. "Max"), or `None` when unknown.
    pub plan: Option<String>,
    pub gauges: Vec<Gauge>,
    /// Index into `gauges` of the Driving Gauge, or `None`.
    pub driving_idx: Option<usize>,
    pub mood: Option<Mood>,
    pub status: Status,
    /// Human reason for a non-Normal status — the Stale warning ("rate limited",
    /// "offline") or the Blind cause ("session expired"). `None` when Normal.
    pub status_note: Option<String>,
    /// Epoch millis when a rate-limited retry is allowed (from the 429
    /// `Retry-After`), so the Popover can count down and the poll can back off.
    /// `None` unless we're rate-limited with a known retry time.
    pub retry_at: Option<i64>,
    /// Epoch millis of the last successful fetch, or `None` before the first one.
    pub fetched_at: Option<i64>,
}

impl Snapshot {
    /// The startup state: the Eye is asleep until the first successful fetch.
    pub fn asleep() -> Self {
        Snapshot {
            plan: None,
            gauges: Vec::new(),
            driving_idx: None,
            mood: None,
            status: Status::Asleep,
            status_note: None,
            retry_at: None,
            fetched_at: None,
        }
    }
}
