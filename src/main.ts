// Popover bootstrap. Two entry paths share one renderer:
//   • Real app (Tauri present) — live Snapshots over IPC (`get_snapshot` +
//     `snapshot-updated`), plus the in-Popover Refresh button wired over IPC by
//     one delegated click listener. While a manual refresh is in flight the
//     button spins until the fresh snapshot (or a timeout fallback) clears it.
//   • Plain browser / dobby dev URL — mock Snapshot keyed by ?state=. Refresh
//     stays inert (no invoke, no setSize), but selecting a gauge updates the mock
//     locally so the click-to-drive interaction is demonstrable without Tauri.

import { renderPopover } from "./popover/render";
import {
	mockMoodFor,
	mockSnapshot,
	type Snapshot,
	type Status,
} from "./popover/snapshot";

const root = document.querySelector<HTMLElement>("#app");
if (!root) {
	throw new Error("mad-eye: missing #app mount node");
}

const runtime = window as Window & {
	__TAURI__?: unknown;
	__TAURI_INTERNALS__?: unknown;
};

if (runtime.__TAURI__ || runtime.__TAURI_INTERNALS__) {
	// Real app: render the current Snapshot at once, then re-render on every push.
	// The 180s poll and the Popover's Refresh button emit `snapshot-updated`,
	// which updates the panel live while it's on screen.
	const { invoke } = await import("@tauri-apps/api/core");
	const { listen } = await import("@tauri-apps/api/event");
	const { getCurrentWindow, LogicalSize } = await import(
		"@tauri-apps/api/window"
	);

	// The frosted window must hug the card so there's no empty vibrancy margin:
	// after every render, measure the card and resize the OS window to it (fixed
	// width, measured height). The card is shorter now that Refresh is its only
	// control, so setSize MUST re-measure on every paint. tao anchors a resize at
	// the top-left, so the panel keeps sitting just under the Eye and grows
	// downward. This block runs ONLY under Tauri — the dev-URL path never resizes.
	const POPOVER_WIDTH = 360;

	// A manual refresh that hangs (no `snapshot-updated` ever arrives) must not
	// leave the button spinning forever — clear it after this grace period.
	const REFRESH_TIMEOUT_MS = 10_000;

	// `refreshing` is app state, not usage data, so it lives here (outside the
	// Snapshot). `current` is the last Snapshot drawn, so a refresh click can
	// repaint the spinner over it at once. Seeded so an early paint has data
	// (never actually shown — replaced by `get_snapshot` on load).
	let refreshing = false;
	let current: Snapshot = mockSnapshot("asleep");
	let refreshTimeout: number | undefined;

	// Draw `snap` and size the OS window to the card (content-hug). Reads the
	// module `refreshing` flag so the Refresh button spins/settles in step.
	const paint = (snap: Snapshot): void => {
		current = snap;
		renderPopover(root, snap, { refreshing });
		const card = root.querySelector<HTMLElement>(".popover");
		const height = Math.ceil(
			card?.getBoundingClientRect().height ?? root.scrollHeight,
		);
		if (height > 0) {
			void getCurrentWindow().setSize(new LogicalSize(POPOVER_WIDTH, height));
		}
	};

	// Stop the spinner and cancel any pending fallback. Called when a fresh
	// snapshot arrives and by the fallback timeout itself.
	const stopRefreshing = (): void => {
		if (refreshTimeout !== undefined) {
			clearTimeout(refreshTimeout);
			refreshTimeout = undefined;
		}
		refreshing = false;
	};

	// Start a manual refresh: spin the button, invoke the fetch, and arm a
	// fallback so a hung fetch can't leave the spinner stuck.
	const startRefresh = (): void => {
		// Ignore repeat clicks while a fetch is already in flight.
		if (refreshing) {
			return;
		}
		refreshing = true;
		paint(current); // show the spinner immediately, over the current data
		void invoke("refresh_now_cmd");
		// Fallback: a hung fetch never emits `snapshot-updated`, so drop the
		// spinner after a grace period rather than spinning forever.
		refreshTimeout = window.setTimeout(() => {
			stopRefreshing();
			paint(current);
		}, REFRESH_TIMEOUT_MS);
	};

	// Make the activated Gauge drive the Eye. Rust persists the choice, recomputes
	// the current Snapshot's driver + Mood (NO fetch), and emits `snapshot-updated`,
	// which repaints the panel and updates the Eye instantly.
	const selectDriver = (control: HTMLElement): void => {
		const name = control.dataset.name;
		if (name !== undefined) {
			void invoke("set_driver", { name });
		}
	};

	// ONE delegated listener on the stable root. renderPopover replaces root's
	// children (never root itself), so this handler survives every re-render and
	// keeps both the Refresh button and every gauge row wired. Dispatch on the
	// nearest data-action ancestor.
	root.addEventListener("click", (event) => {
		const origin = event.target as HTMLElement | null;
		const control = origin?.closest<HTMLElement>("[data-action]");
		const action = control?.dataset.action;
		if (action === "refresh") {
			startRefresh();
		} else if (action === "select-driver" && control) {
			selectDriver(control);
		}
	});

	// The gauge rows are role="button" + tabindex, so activate them by keyboard
	// too (Enter/Space); the Refresh button is a real <button> and needs none.
	root.addEventListener("keydown", (event) => {
		if (event.key !== "Enter" && event.key !== " ") {
			return;
		}
		const origin = event.target as HTMLElement | null;
		const control = origin?.closest<HTMLElement>(
			"[data-action='select-driver']",
		);
		if (control) {
			event.preventDefault();
			selectDriver(control);
		}
	});

	paint(await invoke<Snapshot>("get_snapshot"));
	await listen<Snapshot>("snapshot-updated", (event) => {
		// The fetch returned: cancel the fallback, drop the spinner, and draw the
		// fresh Snapshot (which re-renders the button back to the static icon).
		stopRefreshing();
		paint(event.payload);
	});
} else {
	const requested = new URLSearchParams(window.location.search).get("state");
	// The Refresh button renders (for layout verification) but stays inert — no
	// invoke and no setSize on this path. Gauge selection, however, IS wired
	// locally so the click-to-drive interaction is demonstrable in a plain browser:
	// activating a gauge re-points `drivingIdx` and recomputes the Mood, mirroring
	// what Rust's set_driver would emit.
	let snap = mockSnapshot(coerceStatus(requested));
	renderPopover(root, snap);

	const selectDriver = (control: HTMLElement): void => {
		const name = control.dataset.name;
		if (name === undefined) {
			return;
		}
		const gauge = snap.gauges.find((candidate) => candidate.name === name);
		if (gauge === undefined) {
			return;
		}
		snap = {
			...snap,
			drivingIdx: snap.gauges.indexOf(gauge),
			mood: mockMoodFor(gauge.utilization),
		};
		renderPopover(root, snap);
	};

	root.addEventListener("click", (event) => {
		const origin = event.target as HTMLElement | null;
		const control = origin?.closest<HTMLElement>(
			"[data-action='select-driver']",
		);
		if (control) {
			selectDriver(control);
		}
	});

	root.addEventListener("keydown", (event) => {
		if (event.key !== "Enter" && event.key !== " ") {
			return;
		}
		const origin = event.target as HTMLElement | null;
		const control = origin?.closest<HTMLElement>(
			"[data-action='select-driver']",
		);
		if (control) {
			event.preventDefault();
			selectDriver(control);
		}
	});
}

/** Map a raw `?state=` value to a Status, defaulting to "normal". */
function coerceStatus(value: string | null): Status {
	if (
		value === "normal" ||
		value === "stale" ||
		value === "blind" ||
		value === "asleep"
	) {
		return value;
	}
	return "normal";
}
