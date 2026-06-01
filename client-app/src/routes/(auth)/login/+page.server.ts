import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { ROUTES } from '$lib/routes';

export const load: PageServerLoad = async ({ cookies }) => {
	if (cookies.get('token')) redirect(302, ROUTES.HOME);
	return {};
};

export const actions: Actions = {
	default: async ({ request, cookies, fetch }) => {
		const data = await request.formData();
		const email = data.get('email') as string;
		const password = data.get('password') as string;

		if (!email || !password) {
			return fail(422, { error: 'Email and password are required.', email });
		}

		const res = await fetch(
			`${process.env.API_URL ?? 'http://localhost:8080'}/api/auth/sessions`,
			{
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ email, password })
			}
		);

		if (!res.ok) {
			const body = await res.json().catch(() => ({}));
			const code = body?.error?.code ?? '';
			const messages: Record<string, string> = {
				account_suspended: 'Your account has been suspended.',
				account_locked: 'Too many failed attempts. Try again in 15 minutes.',
				email_not_verified: 'Please verify your email before signing in.',
				unauthorized: 'Invalid email or password.'
			};
			return fail(res.status, {
				error: messages[code] ?? 'Sign in failed. Please try again.',
				email
			});
		}

		const json = await res.json();
		const token = json.data?.access_token;
		const refreshToken = json.data?.refresh_token;

		if (token) {
			cookies.set('token', token, {
				httpOnly: true,
				sameSite: 'lax',
				secure: process.env.NODE_ENV === 'production',
				path: '/',
				maxAge: 3600
			});
		}
		if (refreshToken) {
			cookies.set('refresh_token', refreshToken, {
				httpOnly: true,
				sameSite: 'lax',
				secure: process.env.NODE_ENV === 'production',
				path: '/',
				maxAge: 604800
			});
		}

		redirect(302, ROUTES.HOME);
	}
};
