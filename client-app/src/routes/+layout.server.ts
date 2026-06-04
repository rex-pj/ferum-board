import type { LayoutServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: LayoutServerLoad = async ({ fetch, depends }) => {
	depends('app:config');
	const res = await fetch(`${API}/api/public-config`).catch(() => null);
	const cfg: Record<string, string> = res?.ok ? ((await res.json()).data ?? {}) : {};
	return {
		siteName: cfg.site_name ?? 'Ferum Board',
		siteSlogan: cfg.site_slogan ?? null,
		siteDescription: cfg.site_tagline ?? null,
		logoUrl: cfg.logo_url ?? null,
		faviconUrl: cfg.favicon_url ?? null,
		primaryColor: cfg.primary_color ?? null
	};
};
