import { redirect } from '@sveltejs/kit';
import type { LayoutServerLoad } from './$types';
import { ROUTES } from '$lib/routes';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: LayoutServerLoad = async ({ cookies, fetch }) => {
	if (cookies.get('token')) redirect(302, ROUTES.HOME);

	const cfgRes = await fetch(`${API}/api/public-config`).catch(() => null);
	const cfgJson = cfgRes?.ok ? await cfgRes.json() : { data: {} };
	const cfg: Record<string, string> = cfgJson.data ?? {};

	return {
		siteName: cfg.site_name ?? 'Ferum Board',
		logoUrl: cfg.logo_url ?? null
	};
};
