import { redirect } from '@sveltejs/kit';
import type { LayoutServerLoad } from './$types';
import { ROUTES } from '$lib/routes';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: LayoutServerLoad = async ({ cookies, fetch, depends, parent }) => {
	depends('app:user', 'app:categories', 'app:preferences');
	const token = cookies.get('token');
	if (!token) redirect(302, ROUTES.LOGIN);

	const [{ siteName, siteSlogan, siteDescription, logoUrl, primaryColor }, userRes, catRes, prefsRes] = await Promise.all([
		parent(),
		fetch(`${API}/api/users/me`, { headers: { Authorization: `Bearer ${token}` } }).catch(() => null),
		fetch(`${API}/api/categories`).catch(() => null),
		fetch(`${API}/api/users/me/preferences`, { headers: { Authorization: `Bearer ${token}` } }).catch(() => null)
	]);

	if (!userRes?.ok) redirect(302, ROUTES.LOGIN);

	const userJson = await userRes!.json();
	const catJson = catRes?.ok ? await catRes.json() : { data: [] };
	const prefsJson = prefsRes?.ok ? await prefsRes.json() : { data: null };

	return {
		user: userJson.data,
		categories: catJson.data ?? [],
		siteName,
		siteSlogan,
		siteDescription,
		logoUrl,
		primaryColor,
		theme: prefsJson.data?.theme ?? null
	};
};
