import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { apiFetch } from '$lib/server/api';
import type { ApiResponse } from '$lib/server/api';

export interface SubcategoryIndex {
	id: string;
	slug: string;
	name: string;
	description?: string | null;
	color?: string | null;
	thread_count: number;
}

export interface ForumIndexGroup {
	id: string;
	slug: string;
	name: string;
	description?: string | null;
	color?: string | null;
	position: number;
	thread_count: number;
	subcategories: SubcategoryIndex[];
	recent_threads: any[];
}

export const load: PageServerLoad = async ({ cookies }) => {
	try {
		const res = await apiFetch<ApiResponse<ForumIndexGroup[]>>('/api/forum-index', { cookies });
		return { groups: res.data };
	} catch {
		error(500, 'Failed to load forum index');
	}
};
