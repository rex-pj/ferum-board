import { redirect } from '@sveltejs/kit';
import type { LayoutServerLoad } from './$types';
import { ROUTES } from '$lib/routes';

export const load: LayoutServerLoad = async ({ cookies, parent }) => {
	if (cookies.get('token')) redirect(302, ROUTES.HOME);
	const { siteName, siteSlogan, logoUrl } = await parent();
	return { siteName, siteSlogan, logoUrl };
};
