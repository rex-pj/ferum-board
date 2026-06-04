import { error, fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ params, url, cookies, fetch }) => {
	const page = parseInt(url.searchParams.get('page') ?? '1') || 1;
	const token = cookies.get('token');
	const authHeaders: Record<string, string> = token ? { Authorization: `Bearer ${token}` } : {};

	const threadRes = await fetch(`${API}/api/threads/${params.slug}`, { headers: authHeaders });
	if (!threadRes.ok) error(threadRes.status === 404 ? 404 : 500, 'Thread not found');

	const threadJson = await threadRes.json();
	const thread = threadJson.data;

	const bookmarkStatusFetch = token
		? fetch(`${API}/api/threads/${thread.id}/bookmarks`, { headers: authHeaders })
		: Promise.resolve(null);

	const [postsRes, catRes, bookmarkStatusRes] = await Promise.all([
		fetch(`${API}/api/threads/${thread.id}/posts?page=${page}&per_page=20`, { headers: authHeaders }),
		fetch(`${API}/api/categories/${thread.category_slug}`, { headers: authHeaders }),
		bookmarkStatusFetch
	]);

	const [postsJson, catJson] = await Promise.all([
		postsRes.ok ? postsRes.json() : { data: [], meta: {} },
		catRes.ok ? catRes.json() : { data: null }
	]);

	let isBookmarked = false;
	if (bookmarkStatusRes?.ok) {
		const bmJson = await bookmarkStatusRes.json();
		isBookmarked = bmJson.data?.bookmarked ?? false;
	}

	const firstPost = postsJson.data?.[0];
	const excerpt = firstPost
		? firstPost.content_md.slice(0, 160).replace(/\n/g, ' ')
		: '';

	return {
		thread,
		posts: postsJson.data ?? [],
		postsMeta: postsJson.meta ?? { total: 0, page, per_page: 20 },
		category: catJson.data,
		excerpt,
		isBookmarked,
		canonicalUrl: url.href
	};
};

export const actions: Actions = {
	reply: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { error: 'You must be signed in to reply.' });

		const data = await request.formData();
		const content_md = data.get('content_md') as string;
		const thread_id = data.get('thread_id') as string;
		const parent_id = (data.get('parent_id') as string) || null;

		if (!content_md?.trim()) return fail(422, { error: 'Reply content is required.' });
		if (!thread_id) return fail(422, { error: 'Thread ID is missing.' });

		const res = await fetch(`${API}/api/threads/${thread_id}/posts`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ content_md, ...(parent_id ? { parent_id } : {}) })
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			const code = err?.error?.code ?? '';
			const messages: Record<string, string> = {
				thread_locked: 'This thread is locked.',
				trust_level_insufficient: 'Your trust level is too low to post here.',
				account_suspended: 'Your account is suspended.'
			};
			return fail(res.status, { error: messages[code] ?? 'Failed to post reply.' });
		}

		return {};
	},

	pin: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { error: 'Unauthorized' });

		const data = await request.formData();
		const thread_id = data.get('thread_id') as string;
		const is_pinned = data.get('is_pinned') === 'true';
		if (!thread_id) return fail(422, { error: 'Thread ID is missing.' });

		await fetch(`${API}/api/threads/${thread_id}/pin`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ pinned: !is_pinned })
		});

		return {};
	},

	lock: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { error: 'Unauthorized' });

		const data = await request.formData();
		const thread_id = data.get('thread_id') as string;
		const thread_status = data.get('thread_status') as string;
		if (!thread_id) return fail(422, { error: 'Thread ID is missing.' });

		const locked = thread_status !== 'locked';
		await fetch(`${API}/api/threads/${thread_id}/lock`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ locked })
		});

		return {};
	},

	bookmark: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { error: 'Unauthorized' });

		const data = await request.formData();
		const thread_id = data.get('thread_id') as string;
		if (!thread_id) return fail(422, { error: 'Thread ID is missing.' });

		await fetch(`${API}/api/threads/${thread_id}/bookmarks`, {
			method: 'POST',
			headers: { Authorization: `Bearer ${token}` }
		});

		return { bookmarked: true };
	},

	unbookmark: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { error: 'Unauthorized' });

		const data = await request.formData();
		const thread_id = data.get('thread_id') as string;
		if (!thread_id) return fail(422, { error: 'Thread ID is missing.' });

		await fetch(`${API}/api/threads/${thread_id}/bookmarks`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});

		return { bookmarked: false };
	},

	markBestAnswer: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { error: 'Unauthorized' });

		const data = await request.formData();
		const post_id = data.get('post_id') as string;
		const thread_id = data.get('thread_id') as string;
		if (!post_id) return fail(422, { error: 'post_id is required.' });
		if (!thread_id) return fail(422, { error: 'Thread ID is missing.' });

		const res = await fetch(`${API}/api/threads/${thread_id}/solve`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ best_answer_id: post_id })
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to mark best answer.' });
		}

		return {};
	},

	uploadThumbnail: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { thumbnailError: 'Unauthorized' });

		const data = await request.formData();
		const thread_id = data.get('thread_id') as string;
		const file = data.get('file') as File | null;

		if (!thread_id) return fail(422, { thumbnailError: 'Thread ID is missing.' });
		if (!file || file.size === 0) return fail(422, { thumbnailError: 'No file selected.' });

		const forwardForm = new FormData();
		forwardForm.append('file', file);

		const res = await fetch(`${API}/api/threads/${thread_id}/thumbnail`, {
			method: 'POST',
			headers: { Authorization: `Bearer ${token}` },
			body: forwardForm
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { thumbnailError: err?.error?.message ?? 'Upload failed.' });
		}

		const body = await res.json();
		return { thumbnailUrl: body.data?.thumbnail_url ?? null };
	},

	removeThumbnail: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { thumbnailError: 'Unauthorized' });

		const data = await request.formData();
		const thread_id = data.get('thread_id') as string;
		if (!thread_id) return fail(422, { thumbnailError: 'Thread ID is missing.' });

		const res = await fetch(`${API}/api/threads/${thread_id}/thumbnail`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});

		if (!res.ok) return fail(res.status, { thumbnailError: 'Failed to remove thumbnail.' });

		return { thumbnailRemoved: true };
	}
};
