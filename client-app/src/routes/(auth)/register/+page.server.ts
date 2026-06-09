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
			const message: string = body?.error?.message ?? 'Registration failed. Please try again.';

			if (code === 'email_taken') {
				return fail(res.status, { fieldErrors: { email: 'That email address is already registered.' }, username, email });
			}
			if (code === 'username_taken') {
				return fail(res.status, { fieldErrors: { username: 'That username is taken. Please choose another.' }, username, email });
			}
			// Backend validation errors mention the field name in the message
			if (/username/i.test(message)) {
				return fail(res.status, { fieldErrors: { username: message }, username, email });
			}
			if (/password/i.test(message)) {
				return fail(res.status, { fieldErrors: { password: message }, username, email });
			}
			if (/email/i.test(message)) {
				return fail(res.status, { fieldErrors: { email: message }, username, email });
			}
			return fail(res.status, { error: message, username, email });
		}

		return { success: true };
	}
};
