// The Popover renders purely from a Snapshot — this is the seam.
//
// In the real app (task 5) a Snapshot arrives from Rust over IPC. Here in the
// dev path it comes from `mockSnapshot(?state=)`, so the whole UI is verifiable
// in a plain browser with no Tauri. This type mirrors the Rust `Snapshot`
// (usage/types.rs) field-for-field, camelCased.

/** How agitated the Eye is — a pure function of the driving Gauge %. */
export type Mood = "calm" | "nervous" | "paranoid" | "frantic" | "shattered";

/** Health of the reading behind a Snapshot (mirrors Rust `Status`). */
export type Status = "normal" | "stale" | "blind" | "asleep";

/** Which usage limit a Gauge measures (mirrors Rust `GaugeKind`). */
type GaugeKind = "session" | "weeklyAll" | "weeklyScoped";

/** One usage-limit reading: percent used plus when its window resets. */
export interface Gauge {
  kind: GaugeKind;
  /** Human label, already localized upstream (e.g. "Weekly · Fable"). */
  name: string;
  /** ISO-8601 instant the window resets. */
  resetsAt: string;
  /** Percent used, 0–100. */
  utilization: number;
}

/** Everything the Popover needs to draw one frame. */
export interface Snapshot {
  /** Index into `gauges` of the Gauge that drives the Eye, or null. */
  drivingIdx: number | null;
  /** Epoch ms of the last successful fetch, or null before the first one. */
  fetchedAt: number | null;
  gauges: Gauge[];
  mood: Mood | null;
  /** Plan label (e.g. "Max 20x"), or null when unknown. */
  plan: string | null;
  /** Epoch ms when a rate-limited retry is allowed (from the 429 Retry-After);
   * drives the live countdown + the disabled Refresh. null unless rate-limited. */
  retryAt: number | null;
  status: Status;
  /** Human reason for a non-normal status: the stale warning ("rate limited",
   * "offline") or the blind cause ("session expired"). null when normal. */
  statusNote: string | null;
}

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;

/** ISO string for `ms` from now — keeps the mock's countdowns live. */
function isoIn(ms: number): string {
  return new Date(Date.now() + ms).toISOString();
}

/** The representative gauge set the normal/stale/blind mocks share — mirrors the
 * real `limits[]` shape: one session gauge plus the all-models and Fable (scoped)
 * weeklies, values from a live 2026-07-22 response (26 / 13 / 9). */
function sampleGauges(): Gauge[] {
  return [
    {
      kind: "session",
      name: "Session",
      resetsAt: isoIn(2 * HOUR + 45 * MINUTE),
      utilization: 26,
    },
    {
      kind: "weeklyAll",
      name: "Weekly · all models",
      resetsAt: isoIn(5 * HOUR + 10 * MINUTE),
      utilization: 13,
    },
    {
      kind: "weeklyScoped",
      name: "Weekly · Fable",
      resetsAt: isoIn(5 * HOUR + 40 * MINUTE),
      utilization: 9,
    },
  ];
}

/**
 * Mirror of Rust `mood_for` (usage/mood.rs) for the dev-URL mock: map a driving %
 * to its Mood band, so clicking a Gauge in the browser demo updates the Eye Mood
 * exactly as the native app would. Bands: <50 calm · <80 nervous · <95 paranoid ·
 * <100 frantic · >=100 shattered.
 */
export function mockMoodFor(pct: number): Mood {
  if (pct < 50) {
    return "calm";
  }
  if (pct < 80) {
    return "nervous";
  }
  if (pct < 95) {
    return "paranoid";
  }
  if (pct < 100) {
    return "frantic";
  }
  return "shattered";
}

/**
 * Representative Snapshot for each Status, keyed by `?state=` on the dev URL.
 * The driving Gauge defaults to the Session gauge (index 0), matching the real
 * app's new default; its 26% => a Calm Mood (mood_for bands). Clicking another
 * gauge in the browser demo re-points `drivingIdx` (see main.ts + mockMoodFor).
 */
export function mockSnapshot(state: Status): Snapshot {
  switch (state) {
    case "normal":
      return {
        drivingIdx: 0,
        fetchedAt: Date.now() - 60 * 1000,
        gauges: sampleGauges(),
        mood: "calm",
        plan: "Max 20x",
        retryAt: null,
        status: "normal",
        statusNote: null,
      };
    case "stale":
      return {
        drivingIdx: 0,
        fetchedAt: Date.now() - 8 * MINUTE,
        gauges: sampleGauges(),
        mood: "calm",
        plan: "Max 20x",
        retryAt: Date.now() + 2 * MINUTE + 7 * 1000,
        status: "stale",
        statusNote: "rate limited",
      };
    case "blind":
      // Blind keeps the last known plan/gauges, but the render shows only
      // the cause — the Eye can't see, so the numbers are untrustworthy.
      return {
        drivingIdx: 0,
        fetchedAt: Date.now() - 12 * MINUTE,
        gauges: sampleGauges(),
        mood: null,
        plan: "Max 20x",
        retryAt: null,
        status: "blind",
        statusNote: "session expired",
      };
    case "asleep":
      return {
        drivingIdx: null,
        fetchedAt: null,
        gauges: [],
        mood: null,
        plan: null,
        retryAt: null,
        status: "asleep",
        statusNote: null,
      };
    default:
      return {
        drivingIdx: null,
        fetchedAt: null,
        gauges: [],
        mood: null,
        plan: null,
        retryAt: null,
        status: "asleep",
        statusNote: null,
      };
  }
}
