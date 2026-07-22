//! The usage endpoint adapter: fetch `/api/oauth/usage` and parse its `limits[]`
//! JSON into Gauges (with a fallback to the legacy per-window shape).
//!
//! `parse_usage` is pure (the tested body half); `fetch_usage` is the thin ureq
//! wrapper around it. NEVER log the token — only the parsed Gauges/status leave
//! this module.

use serde::Deserialize;

use super::types::{Gauge, GaugeKind};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const ANTHROPIC_BETA: &str = "oauth-2025-04-20";
const USER_AGENT: &str = "claude-code/2.1.216";

/// Why a usage fetch failed. The poll loop maps each variant to a Snapshot state.
#[derive(Debug)]
pub enum FetchError {
    /// 401 — the stored token is expired/invalid. We never refresh (Decision #4).
    Unauthorized,
    /// 429 — rate limited. Kept as Stale, not Blind. Carries the `Retry-After`
    /// delta-seconds when the server sends it, so the Popover can say when to
    /// retry; `None` when the header is absent/unparseable.
    RateLimited(Option<u64>),
    /// Any other non-2xx status.
    Http(u16),
    /// Transport / IO error reaching the endpoint.
    Network(String),
    /// The body was not the JSON shape we expected.
    Parse(String),
}

/// The relevant slice of the endpoint response. The current shape carries every
/// per-model limit in `limits[]`; the pre-2026-07 shape used top-level
/// `five_hour`/`seven_day*` windows, still read as a fallback when `limits` is
/// empty/absent. Unknown fields (e.g. `extra_usage`) are ignored.
#[derive(Deserialize)]
struct UsageResponse {
    #[serde(default)]
    limits: Vec<LimitEntry>,
    // Legacy top-level windows — only consulted when `limits` is empty/absent.
    #[serde(default)]
    five_hour: Option<Window>,
    #[serde(default)]
    seven_day: Option<Window>,
    #[serde(default)]
    seven_day_opus: Option<Window>,
    #[serde(default)]
    seven_day_sonnet: Option<Window>,
}

/// One entry of the current `limits[]` array. `percent` is ALREADY a 0–100
/// percentage (no scale guard); `scope` is present only for `weekly_scoped` and
/// names the model the limit applies to.
#[derive(Deserialize)]
struct LimitEntry {
    kind: String,
    percent: f64,
    resets_at: String,
    #[serde(default)]
    scope: Option<Scope>,
}

/// The `scope` of a `weekly_scoped` limit — we only need the model.
#[derive(Deserialize)]
struct Scope {
    #[serde(default)]
    model: Option<ScopeModel>,
}

/// The model a scoped limit applies to; `display_name` is the label we show
/// (e.g. "Fable").
#[derive(Deserialize)]
struct ScopeModel {
    #[serde(default)]
    display_name: Option<String>,
}

/// A legacy top-level usage window (pre-`limits[]` shape). `utilization` may be a
/// 0..1 fraction, so the ×100 scale guard is applied when reading these.
#[derive(Deserialize)]
struct Window {
    utilization: f64,
    resets_at: String,
}

/// Parse the endpoint JSON into ordered Gauges.
///
/// Current shape: map each `limits[]` entry by `kind` — `session` → Session,
/// `weekly_all` → Weekly · all models, `weekly_scoped` → `Weekly · {model}`
/// (model from `scope.model.display_name`, else "model"). `percent` is the
/// authoritative 0–100 utilization (NO scale guard). Unknown kinds are skipped
/// (forward-compatible); the `limits[]` order is preserved.
///
/// Legacy fallback: when `limits` is empty/absent, read the old top-level windows
/// (`five_hour` → Session, `seven_day` → Weekly · all models, `seven_day_opus` →
/// Weekly · Fable, `seven_day_sonnet` → Weekly · Sonnet, both scoped →
/// `WeeklyScoped`), applying the ×100 guard to their 0..1 fractions. Null windows
/// are skipped; an empty response yields an empty Vec (the Popover renders that
/// as "no data"), never an error.
pub fn parse_usage(json: &str) -> Result<Vec<Gauge>, FetchError> {
    let response: UsageResponse =
        serde_json::from_str(json).map_err(|e| FetchError::Parse(e.to_string()))?;

    if response.limits.is_empty() {
        return Ok(gauges_from_legacy(response));
    }

    Ok(response
        .limits
        .into_iter()
        .filter_map(gauge_from_limit)
        .collect())
}

/// Map one `limits[]` entry to a Gauge, or `None` for an unrecognized `kind`
/// (skipped so a future limit type never errors the whole parse). `percent` is
/// used verbatim — it is already a 0–100 percentage.
fn gauge_from_limit(limit: LimitEntry) -> Option<Gauge> {
    let (name, kind) = match limit.kind.as_str() {
        "session" => ("Session".to_string(), GaugeKind::Session),
        "weekly_all" => ("Weekly · all models".to_string(), GaugeKind::WeeklyAll),
        "weekly_scoped" => {
            let model = limit
                .scope
                .as_ref()
                .and_then(|scope| scope.model.as_ref())
                .and_then(|model| model.display_name.as_deref())
                .unwrap_or("model");
            (format!("Weekly · {model}"), GaugeKind::WeeklyScoped)
        }
        _ => return None,
    };
    Some(Gauge {
        name,
        kind,
        utilization: limit.percent,
        resets_at: limit.resets_at,
    })
}

/// Legacy path: map the pre-`limits[]` top-level windows to Gauges in the fixed
/// order, skipping null windows. Both scoped windows (opus → Fable, sonnet)
/// become `WeeklyScoped`.
fn gauges_from_legacy(response: UsageResponse) -> Vec<Gauge> {
    let mut gauges = Vec::new();
    if let Some(window) = response.five_hour {
        gauges.push(gauge_from_window("Session", GaugeKind::Session, window));
    }
    if let Some(window) = response.seven_day {
        gauges.push(gauge_from_window("Weekly · all models", GaugeKind::WeeklyAll, window));
    }
    if let Some(window) = response.seven_day_opus {
        gauges.push(gauge_from_window("Weekly · Fable", GaugeKind::WeeklyScoped, window));
    }
    if let Some(window) = response.seven_day_sonnet {
        gauges.push(gauge_from_window("Weekly · Sonnet", GaugeKind::WeeklyScoped, window));
    }
    gauges
}

/// Build a Gauge from a legacy window, applying the ×100 scale guard (a
/// `utilization <= 1.0` is a 0..1 fraction).
fn gauge_from_window(name: &str, kind: GaugeKind, window: Window) -> Gauge {
    let utilization = if window.utilization <= 1.0 {
        window.utilization * 100.0
    } else {
        window.utilization
    };
    Gauge {
        name: name.to_string(),
        kind,
        utilization,
        resets_at: window.resets_at,
    }
}

/// GET the usage endpoint with the four required headers, map the status, and
/// parse the body. Read-only: the token is only ever a request header, never
/// logged or refreshed.
pub fn fetch_usage(token: &str) -> Result<Vec<Gauge>, FetchError> {
    let authorization = format!("Bearer {token}");
    let mut response = ureq::get(USAGE_URL)
        .config()
        .http_status_as_error(false)
        .build()
        .header("Authorization", authorization.as_str())
        .header("anthropic-beta", ANTHROPIC_BETA)
        .header("User-Agent", USER_AGENT)
        .header("Content-Type", "application/json")
        .call()
        .map_err(|e| FetchError::Network(e.to_string()))?;

    let status = response.status().as_u16();
    match status {
        401 => return Err(FetchError::Unauthorized),
        429 => {
            // Capture `Retry-After` (delta-seconds) so the Popover can say when to
            // retry. Absent/date-form/unparseable → None (we just say "rate limited").
            let retry = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|raw| raw.trim().parse::<u64>().ok());
            return Err(FetchError::RateLimited(retry));
        }
        code if !(200..300).contains(&code) => return Err(FetchError::Http(code)),
        _ => {}
    }

    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| FetchError::Network(e.to_string()))?;

    parse_usage(&body)
}
