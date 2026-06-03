/**
 * Client-side permission helpers.
 *
 * The user object returned by GET /api/users/me includes:
 *   - `roles`: full UserRoleResponse[] (global + category-scoped)
 *   - `primary_role_slug`: slug of the highest-priority global role (for badge display)
 *
 * Permission checking on the frontend is for UI gating only.
 * The backend always re-checks via PermissionChecker — never trust client-side checks alone.
 */

export interface UserRole {
	id: string;
	role: {
		id: string;
		slug: string;
		name: string;
		color?: string;
		is_system: boolean;
		position: number;
	};
	category_id?: string | null;
	expires_at?: string | null;
}

export interface UserWithRoles {
	id: string;
	username: string;
	primary_role_slug?: string | null;
	roles?: UserRole[];
	[key: string]: unknown;
}

// ── Role slug helpers ─────────────────────────────────────────────────────────

export function hasRole(user: UserWithRoles | null | undefined, slug: string): boolean {
	if (!user?.roles) return false;
	return user.roles.some((r) => r.role.slug === slug && !r.category_id);
}

export function isAdmin(user: UserWithRoles | null | undefined): boolean {
	return hasRole(user, 'admin');
}

export function isModerator(user: UserWithRoles | null | undefined): boolean {
	return hasRole(user, 'moderator') || isAdmin(user);
}

/** Returns true if the user has the moderator role scoped to a specific category (or globally). */
export function isModeratorOf(
	user: UserWithRoles | null | undefined,
	categoryId: string
): boolean {
	if (!user?.roles) return false;
	return user.roles.some(
		(r) =>
			r.role.slug === 'moderator' &&
			(r.category_id === categoryId || r.category_id == null) &&
			(!r.expires_at || new Date(r.expires_at) > new Date())
	) || isAdmin(user);
}

/** Returns all category IDs where the user has moderator role. */
export function moderatedCategoryIds(user: UserWithRoles | null | undefined): string[] {
	if (!user?.roles) return [];
	return user.roles
		.filter(
			(r) =>
				r.role.slug === 'moderator' &&
				r.category_id &&
				(!r.expires_at || new Date(r.expires_at) > new Date())
		)
		.map((r) => r.category_id as string);
}

