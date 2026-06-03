import { redirect, error } from '@sveltejs/kit';
import type { LayoutServerLoad } from './$types';
import { ROUTES } from '$lib/routes';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: LayoutServerLoad = async ({ cookies, fetch }) => {
	const token = cookies.get('token');
	if (!token) redirect(302, ROUTES.LOGIN);

	const [userRes, cfgRes] = await Promise.all([
		fetch(`${API}/api/users/me`, { headers: { Authorization: `Bearer ${token}` } }).catch(() => null),
		fetch(`${API}/api/public-config`).catch(() => null)
	]);

	if (!userRes?.ok) redirect(302, ROUTES.LOGIN);

	const json = await userRes!.json();
	const roles: any[] = json.data?.roles ?? [];
	const canMod = roles.some((r) => r.role?.slug === 'moderator' || r.role?.slug === 'admin');
	if (!canMod) {
		error(403, 'Access denied');
	}

	const cfgJson = cfgRes?.ok ? await cfgRes.json() : { data: {} };
	const cfg: Record<string, string> = cfgJson.data ?? {};

	return {
		user: json.data,
		siteName: cfg.site_name ?? 'Ferum Board',
		logoUrl: cfg.logo_url ?? null
	};
};
