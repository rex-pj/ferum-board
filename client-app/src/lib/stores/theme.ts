import { browser } from '$app/environment';
import { writable } from 'svelte/store';

export type Theme = 'auto' | 'light' | 'dark';

const STORAGE_KEY = 'ferum-theme';

function getInitialTheme(): Theme {
	if (!browser) return 'auto';
	return (localStorage.getItem(STORAGE_KEY) as Theme) ?? 'auto';
}

function applyTheme(theme: Theme) {
	if (!browser) return;
	const resolved =
		theme === 'auto'
			? window.matchMedia('(prefers-color-scheme: dark)').matches
				? 'dark'
				: 'light'
			: theme;
	document.documentElement.setAttribute('data-bs-theme', resolved);
}

export const theme = writable<Theme>(getInitialTheme());

theme.subscribe((value) => {
	if (!browser) return;
	localStorage.setItem(STORAGE_KEY, value);
	applyTheme(value);
});

if (browser) {
	window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
		theme.update((t) => {
			if (t === 'auto') applyTheme('auto');
			return t;
		});
	});
}
