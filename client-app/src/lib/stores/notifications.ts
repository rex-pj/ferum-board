import { writable } from 'svelte/store';

export const unreadCount = writable<number>(0);

let eventSource: EventSource | null = null;
let pollInterval: ReturnType<typeof setInterval> | null = null;

async function fetchInitialCount() {
	try {
		const res = await fetch('/api/notifications/unread-count');
		if (res.ok) {
			const json = await res.json();
			unreadCount.set(json.data?.count ?? 0);
		}
	} catch {}
}

export function startSSE() {
	if (eventSource) return;

	// Load current count immediately so the badge is accurate on mount.
	fetchInitialCount();

	eventSource = new EventSource('/api/notifications/stream');

	eventSource.addEventListener('notification', () => {
		// Each server push means a new unread notification arrived.
		unreadCount.update((n) => n + 1);
	});

	eventSource.onerror = () => {
		// Connection dropped — fall back to polling. Skip initial fetch since
		// startSSE() already called it; avoids a duplicate request.
		stopSSE();
		startPolling(60_000, true);
	};
}

export function stopSSE() {
	if (eventSource) {
		eventSource.close();
		eventSource = null;
	}
}

// Polling fallback — used when SSE is unavailable.
export function startPolling(intervalMs = 60_000, skipInitialFetch = false) {
	if (pollInterval) return;
	if (!skipInitialFetch) fetchInitialCount();
	pollInterval = setInterval(async () => {
		try {
			const res = await fetch('/api/notifications/unread-count');
			if (res.ok) {
				const json = await res.json();
				unreadCount.set(json.data?.count ?? 0);
			}
		} catch {}
	}, intervalMs);
}

export function stopPolling() {
	if (pollInterval) {
		clearInterval(pollInterval);
		pollInterval = null;
	}
}

export function stopNotifications() {
	stopSSE();
	stopPolling();
}
