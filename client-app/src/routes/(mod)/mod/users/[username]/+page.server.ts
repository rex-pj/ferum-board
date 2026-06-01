import { error, fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ params, cookies, fetch }) => {
	const token = cookies.get('token')!;

	const res = await fetch(`${API}/api/users/${params.username}`, {
		headers: { Authorization: `Bearer ${token}` }
	});

	if (!res.ok) error(res.status === 404 ? 404 : 500, 'User not found');

	const json = await res.json();
	return { user: json.data };
};

export const actions: Actions = {
	warn: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const id = data.get('id') as string;
		const reason = (data.get('reason') as string)?.trim();

		if (!reason) return fail(422, { warnError: 'Reason is required.' });

		const res = await fetch(`${API}/api/mod/users/${id}/warn`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ reason })
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { warnError: err?.error?.message ?? 'Failed to warn user.' });
		}
		return { warnSuccess: true };
	},

	tempBan: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const id = data.get('id') as string;
		const reason = (data.get('reason') as string)?.trim();
		const until_local = data.get('until') as string;

		if (!reason) return fail(422, { banError: 'Reason is required.' });
		if (!until_local) return fail(422, { banError: 'Ban expiry date is required.' });

		// datetime-local gives "YYYY-MM-DDTHH:MM" — interpret as UTC
		const until = new Date(until_local + ':00Z').toISOString();

		const res = await fetch(`${API}/api/mod/users/${id}/ban`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ reason, until })
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { banError: err?.error?.message ?? 'Failed to ban user.' });
		}
		return { banSuccess: true };
	}
};
