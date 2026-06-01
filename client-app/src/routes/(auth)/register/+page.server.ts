import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { ROUTES } from '$lib/routes';

export const load: PageServerLoad = async ({ cookies }) => {
	if (cookies.get('token')) redirect(302, ROUTES.HOME);
	return {};
};

export const actions: Actions = {
	default: async ({ request, fetch }) => {
		const data = await request.formData();
		const username = data.get('username') as string;
		const email = data.get('email') as string;
		const password = data.get('password') as string;

		if (!username || !email || !password) {
			return fail(422, { error: 'All fields are required.', username, email });
		}

		const res = await fetch(
			`${process.env.API_URL ?? 'http://localhost:8080'}/api/auth/registrations`,
			{
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ username, email, password })
			}
		);

		if (!res.ok) {
			const body = await res.json().catch(() => ({}));
			const code = body?.error?.code ?? '';
			const messages: Record<string, string> = {
				email_taken: 'That email address is already registered.',
				username_taken: 'That username is taken. Please choose another.',
				validation_error: body?.error?.message ?? 'Invalid input.'
			};
			return fail(res.status, {
				error: messages[code] ?? 'Registration failed. Please try again.',
				username,
				email
			});
		}

		return { success: true };
	}
};
