import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch }) => {
	const token = cookies.get('token')!;
	const headers = { Authorization: `Bearer ${token}` };

	const [configRes, webhooksRes] = await Promise.all([
		fetch(`${API}/api/admin/config`, { headers }),
		fetch(`${API}/api/admin/webhooks`, { headers })
	]);

	const configJson = configRes.ok ? await configRes.json() : { data: {} };
	const webhooksJson = webhooksRes.ok ? await webhooksRes.json() : { data: [] };

	return {
		config: configJson.data ?? {},
		webhooks: webhooksJson.data ?? []
	};
};

export const actions: Actions = {
	save: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();

		const body: Record<string, string> = {};
		for (const [key, value] of data.entries()) {
			if (typeof value === 'string') body[key] = value;
		}

		// Checkboxes are absent from FormData when unchecked — set explicitly
		if (!data.has('registration_open')) body.registration_open = 'false';
		if (!data.has('post_approval_enabled')) body.post_approval_enabled = 'false';

		const res = await fetch(`${API}/api/admin/config`, {
			method: 'PUT',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify(body)
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to save settings.' });
		}
		return { success: 'Settings saved.' };
	},

	createWebhook: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();

		const url = data.get('url') as string;
		const secret = (data.get('secret') as string) || undefined;
		const events = data.getAll('events') as string[];

		if (!url) return fail(422, { error: 'URL is required.' });
		if (events.length === 0) return fail(422, { error: 'Select at least one event.' });

		const res = await fetch(`${API}/api/admin/webhooks`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ url, events, secret })
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to create webhook.' });
		}
		return { success: 'Webhook added.' };
	},

	toggleWebhook: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const id = data.get('id') as string;
		const is_active = data.get('is_active') === 'true';

		await fetch(`${API}/api/admin/webhooks/${id}`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ is_active })
		});
		return {};
	},

	deleteWebhook: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const id = data.get('id') as string;

		await fetch(`${API}/api/admin/webhooks/${id}`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});
		return {};
	},

	uploadLogo: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const file = data.get('file');

		if (!file || !(file instanceof File) || file.size === 0) {
			return fail(422, { error: 'No file selected.' });
		}

		const body = new FormData();
		body.append('file', file);

		const res = await fetch(`${API}/api/admin/config/logo`, {
			method: 'POST',
			headers: { Authorization: `Bearer ${token}` },
			body
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to upload logo.' });
		}
		const json = await res.json();
		return { logoUrl: json.data?.logo_url ?? null };
	},

	removeLogo: async ({ cookies, fetch }) => {
		const token = cookies.get('token')!;
		const res = await fetch(`${API}/api/admin/config/logo`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});
		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to remove logo.' });
		}
		return { logoUrl: null };
	},

	uploadFavicon: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const file = data.get('file');

		if (!file || !(file instanceof File) || file.size === 0) {
			return fail(422, { error: 'No file selected.' });
		}

		const body = new FormData();
		body.append('file', file);

		const res = await fetch(`${API}/api/admin/config/favicon`, {
			method: 'POST',
			headers: { Authorization: `Bearer ${token}` },
			body
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to upload favicon.' });
		}
		const json = await res.json();
		return { faviconUrl: json.data?.favicon_url ?? null };
	},

	removeFavicon: async ({ cookies, fetch }) => {
		const token = cookies.get('token')!;
		const res = await fetch(`${API}/api/admin/config/favicon`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});
		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to remove favicon.' });
		}
		return { faviconUrl: null };
	}
};
