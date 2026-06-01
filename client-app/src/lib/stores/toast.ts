import { writable } from 'svelte/store';

export type ToastKind = 'success' | 'error' | 'warning' | 'info';

export interface ToastMessage {
	id: string;
	kind: ToastKind;
	message: string;
}

function createToastStore() {
	const { subscribe, update } = writable<ToastMessage[]>([]);

	function dismiss(id: string) {
		update((items) => items.filter((t) => t.id !== id));
	}

	function push(kind: ToastKind, message: string, duration: number) {
		const id = Math.random().toString(36).slice(2);
		update((items) => [...items, { id, kind, message }]);
		if (duration > 0) setTimeout(() => dismiss(id), duration);
	}

	return {
		subscribe,
		success: (message: string, duration = 4000) => push('success', message, duration),
		error:   (message: string, duration = 6000) => push('error',   message, duration),
		warning: (message: string, duration = 5000) => push('warning', message, duration),
		info:    (message: string, duration = 4000) => push('info',    message, duration),
		dismiss,
	};
}

export const toast = createToastStore();
