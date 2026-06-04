import { redirect, error } from '@sveltejs/kit';
import type { LayoutServerLoad } from './$types';
import { ROUTES } from '$lib/routes';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: LayoutServerLoad = async ({ cookies, fetch, parent }) => {
	const token = cookies.get('token');
	if (!token) redirect(302, ROUTES.LOGIN);

	const [{ siteName, siteSlogan, logoUrl, primaryColor }, userRes] = await Promise.all([
		parent(),
		fetch(`${API}/api/users/me`, { headers: { Authorization: `Bearer ${token}` } }).catch(() => null)
	]);

	if (!userRes?.ok) redirect(302, ROUTES.LOGIN);

	const json = await userRes!.json();
	const roles: any[] = json.data?.roles ?? [];
	// Check for any global (non-category-scoped) admin.* permission rather than the
	// hardcoded 'admin' slug so that custom admin-equivalent roles are also admitted.
	const isAdmin = roles.some(
		(r) =>
			!r.category_id &&
			(r.permissions as string[] | undefined)?.some((p) => p.startsWith('admin.'))
	);
	if (!isAdmin) {
		error(403, 'Admin access required');
	}

	return { user: json.data, siteName, siteSlogan, logoUrl, primaryColor };
};
