// renderPopover draws one Snapshot into `root`. It is idempotent: call it again
// with a fresh Snapshot to re-render (the live-countdown interval from the
// previous render is cleared first). All user-facing strings are English.
//
// Every render — whatever the Status — ends in the same footer: an "updated X ago"
// line with a Refresh button pushed to the far right. Refresh is the ONLY control
// in the Popover now — Launch at login and Quit live in the tray's right-click
// menu. The button carries a `data-action` so the Tauri host (`main.ts`) can wire
// it with ONE delegated listener on the stable root, which survives re-renders.
// While a manual refresh is in flight (`opts.refreshing`) the button spins in
// place and is disabled. In a plain browser the button still renders but is inert
// (no listener is attached).

import type { Gauge, Snapshot } from "./snapshot";

/** How often live text refreshes while open — 1s so the rate-limit retry
 * countdown ticks by the second (the slower "resets in" / "updated ago" lines
 * just recompute the same value most ticks, which is cheap). */
const TICK_MS = 1000;

const SVG_NS = "http://www.w3.org/2000/svg";

/** Render inputs that are NOT part of the usage Snapshot (app state). */
export interface PopoverOptions {
	/** True while a manual refresh is in flight — the Refresh button spins. */
	refreshing?: boolean;
}

// One live-countdown interval per mounted root, cleared on the next render so
// re-rendering never leaks timers.
const timers = new WeakMap<HTMLElement, number>();

/** Render `snap` into `root`, replacing whatever was there. */
export function renderPopover(
	root: HTMLElement,
	snap: Snapshot,
	opts: PopoverOptions = {},
): void {
	const existing = timers.get(root);
	if (existing !== undefined) {
		clearInterval(existing);
		timers.delete(root);
	}
	root.replaceChildren();

	const panel = el("section", "popover");
	// Each Status swaps only the BODY; the footer below is always the same.
	// `updaters` collects every live-text refresher (reset countdowns + updated-ago).
	const updaters: Array<() => void> = [];

	if (snap.status === "asleep") {
		// User decision: the data area is animated skeleton bars ONLY — no numbers.
		panel.classList.add("popover--skeleton");
		panel.append(renderSkeleton());
	} else if (snap.status === "blind") {
		panel.classList.add("popover--blind");
		panel.append(renderBlind(snap));
	} else {
		if (snap.status === "stale") {
			// Transient failure (rate limit / offline / 5xx): warn clearly. The
			// wording adapts to whether we have a prior reading, and a rate-limit
			// counts down live to its retry time.
			panel.append(renderStaleWarning(snap, updaters));
		}
		if (snap.gauges.length > 0) {
			panel.append(renderGauges(snap, updaters));
		} else {
			// Stale before the FIRST successful reading (e.g. rate-limited from
			// launch): there is nothing to show yet, so a skeleton — not an empty
			// panel — signals "still waiting", matching the honest banner wording.
			panel.append(renderSkeleton());
		}
	}

	panel.append(renderControls(snap, opts, updaters));

	root.append(panel);

	const tick = (): void => {
		for (const update of updaters) {
			update();
		}
	};
	tick();
	const id = window.setInterval(tick, TICK_MS);
	timers.set(root, id);
}

/** The dynamic gauge list; pushes each row's live "resets in" updater. */
function renderGauges(
	snap: Snapshot,
	updaters: Array<() => void>,
): HTMLElement {
	const list = el("ul", "gauges");
	for (const [idx, gauge] of snap.gauges.entries()) {
		const { row, updateReset } = renderGauge(gauge, idx === snap.drivingIdx);
		updaters.push(updateReset);
		list.append(row);
	}
	return list;
}

/** One gauge row plus an updater for its live "resets in" caption. Every row is a
 * click/keyboard target that selects that Gauge as the Eye's driver (`main.ts`
 * dispatches on `data-action="select-driver"` + `data-name`); the driving row
 * wears the accent and a "👁 watching" badge, the rest reveal a "track" hint on
 * hover so the affordance is discoverable. */
function renderGauge(
	gauge: Gauge,
	driving: boolean,
): { row: HTMLElement; updateReset: () => void } {
	const row = el("li", driving ? "gauge gauge--driving" : "gauge");
	// Clickable: selecting the row makes this Gauge drive the Eye. The host
	// (main.ts) reads `data-action`/`data-name`; role + tabindex make it a button.
	row.dataset.action = "select-driver";
	row.dataset.name = gauge.name;
	row.setAttribute("role", "button");
	row.setAttribute("tabindex", "0");
	row.setAttribute(
		"aria-label",
		driving
			? `${gauge.name}, watching — moves the Eye`
			: `Make ${gauge.name} drive the Eye`,
	);

	const top = el("div", "gauge__top");
	top.append(el("span", "gauge__name", gauge.name));
	top.append(el("span", "gauge__pct", formatPct(gauge.utilization)));
	row.append(top);

	const track = el("div", "gauge__track");
	const rounded = Math.round(clampPct(gauge.utilization));
	track.setAttribute("role", "progressbar");
	track.setAttribute("aria-label", gauge.name);
	track.setAttribute("aria-valuemin", "0");
	track.setAttribute("aria-valuemax", "100");
	track.setAttribute("aria-valuenow", String(rounded));
	const fill = el("div", "gauge__fill");
	fill.style.width = `${clampPct(gauge.utilization)}%`;
	track.append(fill);
	row.append(track);

	const meta = el("div", "gauge__meta");
	const reset = el("span", "gauge__reset");
	meta.append(reset);
	if (driving) {
		// The one Gauge whose % moves the Eye — badged, not just accented.
		meta.append(el("span", "gauge__driver", "👁 watching"));
	} else {
		// Discoverability: a faint hint revealed on row hover/focus, so it's clear
		// that clicking selects this Gauge as the driver.
		meta.append(el("span", "gauge__hint", "👁 track"));
	}
	row.append(meta);

	const updateReset = (): void => {
		reset.textContent = formatResetsIn(gauge.resetsAt, Date.now());
	};

	return { row, updateReset };
}

/** Blind body: a muted closed Eye plus the concrete cause. No gauges. */
function renderBlind(snap: Snapshot): HTMLElement {
	const body = el("div", "blind");
	body.append(closedEye());
	body.append(el("p", "blind__title", "Can't read your usage"));
	body.append(el("p", "blind__cause", snap.statusNote ?? "unknown cause"));
	return body;
}

/**
 * Stale warning banner. `lead` reflects whether we have a prior reading to show
 * ("Showing last reading") or not yet ("No data yet"). When the server gave a
 * rate-limit `retryAt`, the trailing text counts down live to it (and flips to
 * "retrying…" at zero); otherwise it's the static reason ("rate limited",
 * "offline"). The live updater is pushed onto the shared 1s tick.
 */
function renderStaleWarning(
	snap: Snapshot,
	updaters: Array<() => void>,
): HTMLElement {
	const banner = el("div", "popover__warning");
	banner.setAttribute("role", "status");
	banner.append(el("span", "popover__warning-icon", "⚠"));
	const lead = snap.gauges.length > 0 ? "Showing last reading" : "No data yet";
	const text = el("span", "popover__warning-text");
	banner.append(text);

	const paint = (): void => {
		if (snap.retryAt != null) {
			const remaining = snap.retryAt - Date.now();
			text.textContent =
				remaining > 0
					? `${lead} · retry in ${formatCountdown(remaining)}`
					: `${lead} · retrying…`;
		} else {
			text.textContent = snap.statusNote
				? `${lead} · ${snap.statusNote}`
				: lead;
		}
	};
	paint();
	updaters.push(paint);
	return banner;
}

/** A rate-limit retry countdown as `M:SS` (ceil to the second): `2:07`, `0:05`. */
function formatCountdown(ms: number): string {
	const total = Math.ceil(ms / 1000);
	const minutes = Math.floor(total / 60);
	const seconds = total % 60;
	return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

/** Asleep body: pulsing skeleton rows only — deliberately textless. */
function renderSkeleton(): HTMLElement {
	const list = el("div", "skeleton");
	list.setAttribute("aria-hidden", "true");
	const barWidths = [78, 60, 68, 52];
	for (const [i, width] of barWidths.entries()) {
		const rowEl = el("div", "skeleton__row");
		const label = el("div", "skeleton__label");
		const bar = el("div", "skeleton__bar");
		bar.style.width = `${width}%`;
		const delay = `${i * 150}ms`;
		label.style.animationDelay = delay;
		bar.style.animationDelay = delay;
		rowEl.append(label, bar);
		list.append(rowEl);
	}
	return list;
}

/**
 * The always-present footer: an "updated X ago" line on the left and the Refresh
 * button pushed to the far right — the Popover's only control (Launch at login +
 * Quit live in the tray's right-click menu). The button carries a `data-action`
 * so the Tauri host wires it with one delegated listener that survives re-renders.
 * While `opts.refreshing` the icon spins and the button is disabled. During a
 * rate-limit cooldown (`snap.retryAt` in the future) Refresh is disabled live —
 * a manual retry would just 429 again — and re-enables the instant it passes.
 * Pushes its live updaters ("updated ago" + the cooldown toggle) onto the tick.
 */
function renderControls(
	snap: Snapshot,
	opts: PopoverOptions,
	updaters: Array<() => void>,
): HTMLElement {
	const controls = el("div", "popover__controls");

	const footer = el("div", "popover__footer");
	const updated = el("span", "popover__updated");
	footer.append(updated);

	const refresh = el("button", "popover__refresh");
	refresh.type = "button";
	refresh.dataset.action = "refresh";
	refresh.setAttribute("aria-label", "Refresh");
	// A centred inline-SVG circular-arrow icon (not a text glyph): it sits
	// dead-centre in the button and — because the icon is centred in its own
	// viewBox — spins cleanly in place while refreshing. A text glyph's ink rides
	// off its baseline, so it looks off-centre and would orbit rather than spin.
	refresh.append(refreshIcon());
	if (opts.refreshing ?? false) {
		refresh.classList.add("is-refreshing");
		refresh.disabled = true;
		refresh.setAttribute("aria-busy", "true");
		refresh.title = "Refreshing…";
	} else {
		// Live cooldown: disabled while the retry window is still open.
		const cooldown = (): void => {
			const waiting = snap.retryAt != null && snap.retryAt - Date.now() > 0;
			refresh.disabled = waiting;
			refresh.classList.toggle("is-cooldown", waiting);
			refresh.title = waiting ? "Rate limited" : "Refresh now";
		};
		cooldown();
		updaters.push(cooldown);
	}
	footer.append(refresh);
	controls.append(footer);

	updaters.push((): void => {
		updated.textContent = formatUpdatedAgo(snap.fetchedAt, Date.now());
	});

	return controls;
}

/** The Eye's closed-lid glyph (matches the tray Eye's blind pose), muted. */
function closedEye(): SVGSVGElement {
	const svg = document.createElementNS(SVG_NS, "svg");
	svg.setAttribute("class", "blind__eye");
	svg.setAttribute("viewBox", "0 0 100 100");
	svg.setAttribute("width", "44");
	svg.setAttribute("height", "44");
	svg.setAttribute("aria-hidden", "true");
	const lid = document.createElementNS(SVG_NS, "line");
	lid.setAttribute("x1", "20");
	lid.setAttribute("y1", "50");
	lid.setAttribute("x2", "80");
	lid.setAttribute("y2", "50");
	lid.setAttribute("stroke", "currentColor");
	lid.setAttribute("stroke-width", "9");
	lid.setAttribute("stroke-linecap", "round");
	svg.append(lid);
	return svg;
}

/**
 * The Refresh control's icon: a circular arrow drawn centred in a 24×24 viewBox
 * (point-symmetric about its centre), so it sits dead-centre in the button and
 * spins cleanly in place when the button carries `.is-refreshing`. Inherits the
 * button's `currentColor`; carries the `popover__refresh-glyph` hook CSS spins.
 */
function refreshIcon(): SVGSVGElement {
	const svg = document.createElementNS(SVG_NS, "svg");
	svg.setAttribute("class", "popover__refresh-glyph");
	svg.setAttribute("viewBox", "0 0 24 24");
	svg.setAttribute("width", "15");
	svg.setAttribute("height", "15");
	svg.setAttribute("fill", "none");
	svg.setAttribute("stroke", "currentColor");
	svg.setAttribute("stroke-width", "2");
	svg.setAttribute("stroke-linecap", "round");
	svg.setAttribute("stroke-linejoin", "round");
	svg.setAttribute("aria-hidden", "true");
	// Two arrowhead corners (top-right, bottom-left) + the two connecting arcs.
	for (const points of ["23 4 23 10 17 10", "1 20 1 14 7 14"]) {
		const head = document.createElementNS(SVG_NS, "polyline");
		head.setAttribute("points", points);
		svg.append(head);
	}
	const arc = document.createElementNS(SVG_NS, "path");
	arc.setAttribute(
		"d",
		"M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15",
	);
	svg.append(arc);
	return svg;
}

/** "resets in Xh Ym" (collapses to Ym under an hour, Xd Yh over a day). */
function formatResetsIn(iso: string, now: number): string {
	const target = Date.parse(iso);
	if (Number.isNaN(target)) {
		return "";
	}
	const ms = target - now;
	if (ms <= 0) {
		return "resets now";
	}
	const totalMinutes = Math.floor(ms / (60 * 1000));
	const days = Math.floor(totalMinutes / (60 * 24));
	const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
	const minutes = totalMinutes % 60;
	if (days > 0) {
		return `resets in ${days}d ${hours}h`;
	}
	if (hours > 0) {
		return `resets in ${hours}h ${minutes}m`;
	}
	return `resets in ${minutes}m`;
}

/** "updated Xm ago", degrading gracefully to seconds / hours / never. */
function formatUpdatedAgo(fetchedAt: number | null, now: number): string {
	if (fetchedAt === null) {
		return "never updated";
	}
	const seconds = Math.floor(Math.max(0, now - fetchedAt) / 1000);
	if (seconds < 10) {
		return "updated just now";
	}
	if (seconds < 60) {
		return `updated ${seconds}s ago`;
	}
	const minutes = Math.floor(seconds / 60);
	if (minutes < 60) {
		return `updated ${minutes}m ago`;
	}
	const hours = Math.floor(minutes / 60);
	return `updated ${hours}h ago`;
}

function formatPct(utilization: number): string {
	return `${Math.round(clampPct(utilization))}%`;
}

function clampPct(value: number): number {
	return Math.min(100, Math.max(0, value));
}

/** Tiny typed element builder — no framework, no barrel. */
function el<K extends keyof HTMLElementTagNameMap>(
	tag: K,
	className?: string,
	text?: string,
): HTMLElementTagNameMap[K] {
	const node = document.createElement(tag);
	if (className !== undefined) {
		node.className = className;
	}
	if (text !== undefined) {
		node.textContent = text;
	}
	return node;
}
