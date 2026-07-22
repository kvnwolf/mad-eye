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
	/** Human label, already localized upstream (e.g. "Weekly · Fable"). */
	name: string;
	kind: GaugeKind;
	/** Percent used, 0–100. */
	utilization: number;
	/** ISO-8601 instant the window resets. */
	resetsAt: string;
}

/** Everything the Popover needs to draw one frame. */
export interface Snapshot {
	/** Plan label (e.g. "Max 20x"), or null when unknown. */
	plan: string | null;
	gauges: Gauge[];
	/** Index into `gauges` of the Gauge that drives the Eye, or null. */
	drivingIdx: number | null;
	mood: Mood | null;
	status: Status;
	/** Human reason for a non-normal status: the stale warning ("rate limited",
	 * "offline") or the blind cause ("session expired"). null when normal. */
	statusNote: string | null;
	/** Epoch ms when a rate-limited retry is allowed (from the 429 Retry-After);
	 * drives the live countdown + the disabled Refresh. null unless rate-limited. */
	retryAt: number | null;
	/** Epoch ms of the last successful fetch, or null before the first one. */
	fetchedAt: number | null;
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
			name: "Session",
			kind: "session",
			utilization: 26,
			resetsAt: isoIn(2 * HOUR + 45 * MINUTE),
		},
		{
			name: "Weekly · all models",
			kind: "weeklyAll",
			utilization: 13,
			resetsAt: isoIn(5 * HOUR + 10 * MINUTE),
		},
		{
			name: "Weekly · Fable",
			kind: "weeklyScoped",
			utilization: 9,
			resetsAt: isoIn(5 * HOUR + 40 * MINUTE),
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
	if (pct < 50) return "calm";
	if (pct < 80) return "nervous";
	if (pct < 95) return "paranoid";
	if (pct < 100) return "frantic";
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
				plan: "Max 20x",
				gauges: sampleGauges(),
				drivingIdx: 0,
				mood: "calm",
				status: "normal",
				statusNote: null,
				retryAt: null,
				fetchedAt: Date.now() - 60 * 1000,
			};
		case "stale":
			return {
				plan: "Max 20x",
				gauges: sampleGauges(),
				drivingIdx: 0,
				mood: "calm",
				status: "stale",
				statusNote: "rate limited",
				retryAt: Date.now() + 2 * MINUTE + 7 * 1000,
				fetchedAt: Date.now() - 8 * MINUTE,
			};
		case "blind":
			// Blind keeps the last known plan/gauges, but the render shows only
			// the cause — the Eye can't see, so the numbers are untrustworthy.
			return {
				plan: "Max 20x",
				gauges: sampleGauges(),
				drivingIdx: 0,
				mood: null,
				status: "blind",
				statusNote: "session expired",
				retryAt: null,
				fetchedAt: Date.now() - 12 * MINUTE,
			};
		case "asleep":
			return {
				plan: null,
				gauges: [],
				drivingIdx: null,
				mood: null,
				status: "asleep",
				statusNote: null,
				retryAt: null,
				fetchedAt: null,
			};
		default:
			return {
				plan: null,
				gauges: [],
				drivingIdx: null,
				mood: null,
				status: "asleep",
				statusNote: null,
				retryAt: null,
				fetchedAt: null,
			};
	}
}
