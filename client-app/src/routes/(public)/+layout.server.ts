import type { LayoutServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: LayoutServerLoad = async ({ cookies, fetch, depends }) => {
	depends('app:categories', 'app:user', 'app:preferences');
	const token = cookies.get('token');

	const [catRes, cfgRes] = await Promise.all([
		fetch(`${API}/api/categories`).catch(() => null),
		fetch(`${API}/api/public-config`).catch(() => null)
	]);
	const catJson = catRes?.ok ? await catRes.json() : { data: [] };
	const cfgJson = cfgRes?.ok ? await cfgRes.json() : { data: {} };
	const categories: { id: string; name: string; slug: string; parent_id?: string | null }[] =
		catJson.data ?? [];
	const cfg: Record<string, string> = cfgJson.data ?? {};
	const siteName = cfg.site_name ?? 'Ferum Board';
	const siteDescription = cfg.site_tagline ?? null;
	const logoUrl = cfg.logo_url ?? null;
	const primaryColor = cfg.primary_color ?? null;

	if (!token) {
		return { user: null, siteName, siteDescription, logoUrl, primaryColor, theme: null, categories };
	}

	try {
		const [userRes, prefsRes] = await Promise.all([
			fetch(`${API}/api/users/me`, { headers: { Authorization: `Bearer ${token}` } }),
			fetch(`${API}/api/users/me/preferences`, { headers: { Authorization: `Bearer ${token}` } })
		]);

		if (!userRes.ok) return { user: null, siteName, siteDescription, logoUrl, primaryColor, theme: null, categories };

		const userJson = await userRes.json();
		const prefsJson = prefsRes.ok ? await prefsRes.json() : { data: null };
		const dbTheme = prefsJson.data?.theme ?? null;

		return { user: userJson.data, siteName, siteDescription, logoUrl, primaryColor, theme: dbTheme, categories };
	} catch {
		return { user: null, siteName, siteDescription, logoUrl, primaryColor, theme: null, categories };
	}
};
