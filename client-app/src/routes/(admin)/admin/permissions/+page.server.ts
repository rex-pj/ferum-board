import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch }) => {
	const token = cookies.get('token')!;
	const [permsRes, rolesRes] = await Promise.all([
		fetch(`${API}/api/admin/roles/permissions`, {
			headers: { Authorization: `Bearer ${token}` }
		}),
		fetch(`${API}/api/admin/roles`, {
			headers: { Authorization: `Bearer ${token}` }
		})
	]);

	const permsJson = permsRes.ok ? await permsRes.json() : { data: [] };
	const rolesJson = rolesRes.ok ? await rolesRes.json() : { data: [] };
	const allRoles: any[] = rolesJson.data ?? [];

	// Fetch role→permissions for each role in parallel so we can show the role matrix
	const rolePermResults = await Promise.all(
		allRoles.map((role) =>
			fetch(`${API}/api/admin/roles/${role.id}/permissions`, {
				headers: { Authorization: `Bearer ${token}` }
			})
				.then((r) => r.json())
				.then((j) => ({ roleId: role.id, keys: (j.data ?? []).map((p: any) => p.key) as string[] }))
				.catch(() => ({ roleId: role.id, keys: [] }))
		)
	);

	// Map: permission key → set of role IDs that have it
	const permToRoles: Record<string, string[]> = {};
	for (const { roleId, keys } of rolePermResults) {
		for (const key of keys) {
			(permToRoles[key] ??= []).push(roleId);
		}
	}

	return {
		permissions: permsJson.data ?? [],
		roles: allRoles,
		permToRoles
	};
};
