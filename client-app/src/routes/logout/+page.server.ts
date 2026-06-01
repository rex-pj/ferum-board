import { redirect } from '@sveltejs/kit';
import type { Actions } from './$types';
import { ROUTES } from '$lib/routes';

export const actions: Actions = {
	default: async ({ cookies, fetch }) => {
		const token = cookies.get('token');
		const refreshToken = cookies.get('refresh_token');

		const cookieHeader = [
			token ? `token=${token}` : null,
			refreshToken ? `refresh_token=${refreshToken}` : null
		]
			.filter(Boolean)
			.join('; ');

		await fetch(`${process.env.API_URL ?? 'http://localhost:8080'}/api/auth/sessions`, {
			method: 'DELETE',
			headers: {
				Authorization: `Bearer ${token ?? ''}`,
				Cookie: cookieHeader
			}
		}).catch(() => {});

		cookies.delete('token', { path: '/' });
		cookies.delete('refresh_token', { path: '/' });
		redirect(302, ROUTES.LOGIN);
	}
};
