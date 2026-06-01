import type { LayoutServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: LayoutServerLoad = async ({ fetch, depends }) => {
	depends('app:favicon');
	const res = await fetch(`${API}/api/public-config`).catch(() => null);
	const json = res?.ok ? await res.json() : { data: {} };
	const faviconUrl: string | null = json.data?.favicon_url || null;
	return { faviconUrl };
};
