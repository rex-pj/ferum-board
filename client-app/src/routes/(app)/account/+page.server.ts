import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch }) => {
	const token = cookies.get('token')!;

	const [profileRes, prefsRes, catRes] = await Promise.all([
		fetch(`${API}/api/users/me`, { headers: { Authorization: `Bearer ${token}` } }),
		fetch(`${API}/api/users/me/preferences`, { headers: { Authorization: `Bearer ${token}` } }),
		fetch(`${API}/api/categories`, { headers: { Authorization: `Bearer ${token}` } })
	]);

	const profile = profileRes.ok ? (await profileRes.json()).data : null;
	const prefs = prefsRes.ok ? (await prefsRes.json()).data : null;
	const allCategories = catRes.ok ? ((await catRes.json()).data ?? []) : [];

	return { profile, prefs, allCategories };
};

export const actions: Actions = {
	updateProfile: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();

		const body: Record<string, string | null> = {};
		const displayName = data.get('display_name') as string;
		const bio = data.get('bio') as string;
		const website = data.get('website') as string;

		if (displayName !== null) body.display_name = displayName || null;
		if (bio !== null) body.bio = bio || null;
		if (website !== null) body.website = website || null;

		const res = await fetch(`${API}/api/users/me`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify(body)
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { profileError: err?.error?.message ?? 'Failed to update profile.' });
		}
		return { profileSuccess: 'Profile updated.' };
	},

	changePassword: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();

		const res = await fetch(`${API}/api/users/me/password`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({
				current_password: data.get('current_password') as string,
				new_password: data.get('new_password') as string
			})
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { passwordError: err?.error?.message ?? 'Failed to change password.' });
		}
		return { passwordSuccess: 'Password changed.' };
	},

	updatePreferences: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();

		const res = await fetch(`${API}/api/users/me/preferences`, {
			method: 'PUT',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({
				theme: data.get('theme') as string,
				font_size: data.get('font_size') as string,
				layout: data.get('layout') as string
			})
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { prefsError: err?.error?.message ?? 'Failed to update preferences.' });
		}
		return { prefsSuccess: 'Preferences saved.' };
	},

	unwatch: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const category_id = data.get('category_id') as string;
		await fetch(`${API}/api/categories/${category_id}/watch`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});
		return { success: true };
	},

	unmute: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const category_id = data.get('category_id') as string;
		await fetch(`${API}/api/categories/${category_id}/mute`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});
		return { success: true };
	}
};
