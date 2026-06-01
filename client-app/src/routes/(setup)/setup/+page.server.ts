import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { ROUTES } from '$lib/routes';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ fetch }) => {
	const res = await fetch(`${API}/api/setup/status`).catch(() => null);
	if (res && res.ok) {
		const json = await res.json().catch(() => null);
		if (json?.data?.needs_setup === false) {
			redirect(302, ROUTES.HOME);
		}
	}
};

export const actions: Actions = {
	run_setup: async ({ request, cookies, fetch }) => {
		const data = await request.formData();
		const raw = data.get('payload');
		if (!raw || typeof raw !== 'string') {
			return fail(422, { error: 'Invalid request payload.' });
		}

		let payload: unknown;
		try {
			payload = JSON.parse(raw);
		} catch {
			return fail(422, { error: 'Could not parse request payload.' });
		}

		const res = await fetch(`${API}/api/setup/run`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify(payload)
		});

		if (!res.ok) {
			const body = await res.json().catch(() => ({}));
			const code = body?.error?.code ?? '';
			const messages: Record<string, string> = {
				setup_already_complete: 'Setup has already been completed.',
				email_taken: 'That email address is already in use.',
				username_taken: 'That username is already in use.'
			};
			return fail(res.status, {
				error: messages[code] ?? body?.error?.message ?? 'Setup failed. Please try again.'
			});
		}

		const json = await res.json();
		const token = json.data?.access_token;
		if (token) {
			cookies.set('token', token, {
				httpOnly: true,
				sameSite: 'lax',
				secure: process.env.NODE_ENV === 'production',
				path: '/',
				maxAge: 3600
			});
		}

		return { success: true };
	}
};
