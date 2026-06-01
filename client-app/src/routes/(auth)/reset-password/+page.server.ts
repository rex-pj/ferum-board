import { fail, redirect } from '@sveltejs/kit';
import type { Actions } from './$types';
import { ROUTES } from '$lib/routes';

export const actions: Actions = {
	default: async ({ request, fetch }) => {
		const data = await request.formData();
		const token = data.get('token') as string;
		const new_password = data.get('new_password') as string;

		if (!token) return fail(422, { error: 'Reset token is missing.' });
		if (!new_password || new_password.length < 8) {
			return fail(422, { error: 'Password must be at least 8 characters.' });
		}

		const res = await fetch(
			`${process.env.API_URL ?? 'http://localhost:8080'}/api/auth/password-resets/${token}`,
			{
				method: 'PATCH',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ new_password })
			}
		);

		if (!res.ok) {
			const body = await res.json().catch(() => ({}));
			const code = body?.error?.code ?? '';
			const messages: Record<string, string> = {
				invalid_or_expired_token: 'This reset link is invalid or has expired.',
				token_already_used: 'This reset link has already been used.'
			};
			return fail(res.status, {
				error: messages[code] ?? 'Failed to reset password. Please try again.'
			});
		}

		return { success: true };
	}
};
