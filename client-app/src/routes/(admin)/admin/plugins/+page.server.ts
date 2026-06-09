import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch }) => {
	const token = cookies.get('token')!;
	const res = await fetch(`${API}/api/admin/plugins`, {
		headers: { Authorization: `Bearer ${token}` }
	});
	const json = res.ok ? await res.json() : { data: [] };
	return { plugins: json.data ?? [] };
};

export const actions: Actions = {
	// Step 1: Upload .fpkg — returns capability review, no DB write yet
	uploadPlugin: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const file = data.get('file') as File | null;

		if (!file || file.size === 0) return fail(422, { error: 'No file selected.' });
		if (!file.name.endsWith('.fpkg')) return fail(422, { error: 'File must be a .fpkg archive.' });

		const formData = new FormData();
		formData.append('file', file);

		const res = await fetch(`${API}/api/admin/plugins/upload`, {
			method: 'POST',
			headers: { Authorization: `Bearer ${token}` },
			body: formData
		});

		const json = await res.json().catch(() => ({}));
		if (!res.ok) return fail(res.status, { error: json?.error?.message ?? 'Upload failed.' });

		return { review: json.data };
	},

	// Step 2: Confirm install — re-upload file with granted capabilities
	installPlugin: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const file = data.get('file') as File | null;
		const grantedJson = (data.get('granted_capabilities') as string) || '{}';

		if (!file || file.size === 0) return fail(422, { error: 'No file selected.' });

		const formData = new FormData();
		formData.append('file', file);
		formData.append('granted_capabilities', grantedJson);

		const res = await fetch(`${API}/api/admin/plugins/install`, {
			method: 'POST',
			headers: { Authorization: `Bearer ${token}` },
			body: formData
		});

		const json = await res.json().catch(() => ({}));
		if (!res.ok) return fail(res.status, { error: json?.error?.message ?? 'Install failed.' });

		return { success: `Plugin "${json.data?.name}" installed.`, plugin: json.data };
	},

	toggleStatus: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const slug = data.get('slug') as string;
		const active = data.get('active') === 'true';

		const res = await fetch(`${API}/api/admin/plugins/${slug}/status`, {
			method: 'PATCH',
			headers: {
				'Content-Type': 'application/json',
				Authorization: `Bearer ${token}`
			},
			body: JSON.stringify({ active })
		});

		const json = await res.json().catch(() => ({}));
		if (!res.ok) return fail(res.status, { error: json?.error?.message ?? 'Status update failed.' });

		return { success: `Plugin ${active ? 'activated' : 'deactivated'}.` };
	},

	uninstall: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const slug = data.get('slug') as string;
		const confirmSlug = data.get('confirm_slug') as string;

		if (slug !== confirmSlug) return fail(422, { error: 'Confirmation slug does not match.' });

		const res = await fetch(`${API}/api/admin/plugins/${slug}`, {
			method: 'DELETE',
			headers: {
				'Content-Type': 'application/json',
				Authorization: `Bearer ${token}`
			},
			body: JSON.stringify({ confirm_slug: confirmSlug })
		});

		if (!res.ok) {
			const json = await res.json().catch(() => ({}));
			return fail(res.status, { error: json?.error?.message ?? 'Uninstall failed.' });
		}

		return { success: 'Plugin uninstalled.' };
	}
};
