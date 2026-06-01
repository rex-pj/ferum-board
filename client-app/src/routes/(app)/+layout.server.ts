import { redirect } from '@sveltejs/kit';
import type { LayoutServerLoad } from './$types';
import { ROUTES } from '$lib/routes';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: LayoutServerLoad = async ({ cookies, fetch, depends }) => {
	depends('app:user', 'app:categories', 'app:preferences');
	const token = cookies.get('token');
	if (!token) redirect(302, ROUTES.LOGIN);

	const [userRes, catRes, prefsRes, cfgRes] = await Promise.all([
		fetch(`${API}/api/users/me`, { headers: { Authorization: `Bearer ${token}` } }).catch(() => null),
		fetch(`${API}/api/categories`).catch(() => null),
		fetch(`${API}/api/users/me/preferences`, { headers: { Authorization: `Bearer ${token}` } }).catch(() => null),
		fetch(`${API}/api/public-config`).catch(() => null)
	]);

	if (!userRes?.ok) redirect(302, ROUTES.LOGIN);

	const userJson = await userRes!.json();
	const catJson = catRes?.ok ? await catRes.json() : { data: [] };
	const prefsJson = prefsRes?.ok ? await prefsRes.json() : { data: null };
	const cfgJson = cfgRes?.ok ? await cfgRes.json() : { data: {} };
	const cfg: Record<string, string> = cfgJson.data ?? {};

	return {
		user: userJson.data,
		categories: catJson.data ?? [],
		siteName: cfg.site_name ?? 'Ferum Board',
		siteDescription: cfg.site_tagline ?? null,
		logoUrl: cfg.logo_url ?? null,
		primaryColor: cfg.primary_color ?? null,
		theme: prefsJson.data?.theme ?? null
	};
};
