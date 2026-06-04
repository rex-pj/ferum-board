const rtf = new Intl.RelativeTimeFormat('en', { numeric: 'auto' });

const THRESHOLDS: [number, Intl.RelativeTimeFormatUnit][] = [
	[60, 'second'],
	[3600, 'minute'],
	[86400, 'hour'],
	[86400 * 30, 'day'],
	[86400 * 365, 'month'],
	[Infinity, 'year']
];

const DIVIDERS: Record<string, number> = {
	second: 1,
	minute: 60,
	hour: 3600,
	day: 86400,
	month: 86400 * 30,
	year: 86400 * 365
};

export function formatRelative(date: Date | string): string {
	const d = typeof date === 'string' ? new Date(date) : date;
	const diff = (d.getTime() - Date.now()) / 1000;
	const abs = Math.abs(diff);

	for (const [threshold, unit] of THRESHOLDS) {
		if (abs < threshold) {
			const value = Math.round(diff / DIVIDERS[unit]);
			return rtf.format(value, unit);
		}
	}
	return formatDate(d);
}

export function formatDate(date: Date | string): string {
	const d = typeof date === 'string' ? new Date(date) : date;
	return d.toLocaleDateString('en', { year: 'numeric', month: 'short', day: 'numeric' });
}

export function formatDateTime(date: Date | string): string {
	const d = typeof date === 'string' ? new Date(date) : date;
	return d.toLocaleString('en', {
		year: 'numeric',
		month: 'short',
		day: 'numeric',
		hour: '2-digit',
		minute: '2-digit'
	});
}

export function truncate(s: string, max: number): string {
	return s.length <= max ? s : s.slice(0, max - 1) + '…';
}

export function formatNumber(n: number): string {
	if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
	if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
	return String(n);
}

const HEX_COLOR_RE = /^#[0-9a-fA-F]{3}([0-9a-fA-F]{3})?$/;
const DEFAULT_TAG_COLOR = '#6c757d';

/**
 * Sanitise a tag color value for use in a CSS custom property.
 * Only hex shorthand (#rgb) and full (#rrggbb) colors are accepted.
 * Any other value — including attempts to inject CSS via semicolons or
 * closing braces — falls back to the default neutral grey.
 */
export function safeCssColor(color: string | null | undefined): string {
	if (!color) return DEFAULT_TAG_COLOR;
	return HEX_COLOR_RE.test(color) ? color : DEFAULT_TAG_COLOR;
}
