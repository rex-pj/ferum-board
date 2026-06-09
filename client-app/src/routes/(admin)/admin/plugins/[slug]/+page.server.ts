import { fail, error } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ params, cookies, fetch }) => {
	const token = cookies.get('token')!;
	const headers = { Authorization: `Bearer ${token}` };

	const [pluginRes, logsRes] = await Promise.all([
		fetch(`${API}/api/admin/plugins/${params.slug}`, { headers }),
		fetch(`${API}/api/admin/plugins/${params.slug}/logs?limit=50`, { headers })
	]);

	if (!pluginRes.ok) {
		throw error(pluginRes.status === 404 ? 404 : 500, 'Plugin not found');
	}

	const pluginJson = await pluginRes.json();
	const logsJson = logsRes.ok ? await logsRes.json() : { data: [] };

	return {
		plugin: pluginJson.data,
		logs: logsJson.data ?? []
	};
};

export const actions: Actions = {
	configure: async ({ request, params, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();

		// Collect all form fields as config object
		const config: Record<string, string | boolean | number> = {};
		for (const [key, value] of data.entries()) {
			if (key === '_action') continue;
			// Try to parse numbers and booleans
			if (value === 'true') config[key] = true;
			else if (value === 'false') config[key] = false;
			else if (value !== '' && !isNaN(Number(value))) config[key] = Number(value);
			else config[key] = value as string;
		}

		const res = await fetch(`${API}/api/admin/plugins/${params.slug}/config`, {
			method: 'PATCH',
			headers: {
				'Content-Type': 'application/json',
				Authorization: `Bearer ${token}`
			},
			body: JSON.stringify({ config })
		});

		if (!res.ok) {
			const json = await res.json().catch(() => ({}));
			return fail(res.status, { error: json?.error?.message ?? 'Failed to save config.' });
		}

		return { success: 'Configuration saved.' };
	},

	activate: async ({ params, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const res = await fetch(`${API}/api/admin/plugins/${params.slug}/status`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ active: true })
		});
		const json = await res.json().catch(() => ({}));
		if (!res.ok) return fail(res.status, { error: json?.error?.message ?? 'Activation failed.' });
		return { success: 'Plugin activated.' };
	},

	deactivate: async ({ params, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const res = await fetch(`${API}/api/admin/plugins/${params.slug}/status`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ active: false })
		});
		const json = await res.json().catch(() => ({}));
		if (!res.ok) return fail(res.status, { error: json?.error?.message ?? 'Deactivation failed.' });
		return { success: 'Plugin deactivated.' };
	}
};
