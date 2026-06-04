import type { LayoutServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: LayoutServerLoad = async ({ cookies, fetch, depends, parent }) => {
	depends('app:categories', 'app:user', 'app:preferences');
	const [{ siteName, siteSlogan, siteDescription, logoUrl, primaryColor }, catRes] = await Promise.all([
		parent(),
		fetch(`${API}/api/categories`).catch(() => null)
	]);
	const categories: { id: string; name: string; slug: string; parent_id?: string | null }[] =
		catRes?.ok ? ((await catRes.json()).data ?? []) : [];

	const token = cookies.get('token');
	if (!token) {
		return { user: null, siteName, siteSlogan, siteDescription, logoUrl, primaryColor, theme: null, categories };
	}

	try {
		const [userRes, prefsRes] = await Promise.all([
			fetch(`${API}/api/users/me`, { headers: { Authorization: `Bearer ${token}` } }),
			fetch(`${API}/api/users/me/preferences`, { headers: { Authorization: `Bearer ${token}` } })
		]);

		if (!userRes.ok) return { user: null, siteName, siteSlogan, siteDescription, logoUrl, primaryColor, theme: null, categories };

		const userJson = await userRes.json();
		const prefsJson = prefsRes.ok ? await prefsRes.json() : { data: null };

		return { user: userJson.data, siteName, siteSlogan, siteDescription, logoUrl, primaryColor, theme: prefsJson.data?.theme ?? null, categories };
	} catch {
		return { user: null, siteName, siteSlogan, siteDescription, logoUrl, primaryColor, theme: null, categories };
	}
};
