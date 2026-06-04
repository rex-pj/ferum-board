import type { RequestHandler } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const GET: RequestHandler = async ({ fetch, url }) => {
	const origin = url.origin;

	const [catRes, threadRes] = await Promise.all([
		fetch(`${API}/api/categories`).catch(() => null),
		fetch(`${API}/api/threads?per_page=1000&sort=newest`).catch(() => null)
	]);

	const catJson = catRes?.ok ? await catRes.json() : { data: [] };
	const threadJson = threadRes?.ok ? await threadRes.json() : { data: [] };

	const categories: { slug: string }[] = catJson.data ?? [];
	const threads: { slug: string; last_post_at?: string; created_at: string }[] = threadJson.data ?? [];

	const staticUrls = [
		{ loc: '/', priority: '1.0', changefreq: 'daily' },
		{ loc: '/forum', priority: '0.9', changefreq: 'daily' }
	];

	const categoryUrls = categories.map((c) => ({
		loc: `/forum/${c.slug}`,
		priority: '0.8',
		changefreq: 'weekly',
		lastmod: undefined as string | undefined
	}));

	const threadUrls = threads.map((t) => ({
		loc: `/forum/t/${t.slug}`,
		priority: '0.7',
		changefreq: 'weekly',
		lastmod: t.last_post_at ?? t.created_at
	}));

	const allUrls = [
		...staticUrls.map((u) => ({ ...u, lastmod: undefined as string | undefined })),
		...categoryUrls,
		...threadUrls
	];

	const xml = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${allUrls
	.map(
		(u) => `  <url>
    <loc>${origin}${u.loc}</loc>${u.lastmod ? `\n    <lastmod>${new Date(u.lastmod).toISOString().split('T')[0]}</lastmod>` : ''}
    <changefreq>${u.changefreq}</changefreq>
    <priority>${u.priority}</priority>
  </url>`
	)
	.join('\n')}
</urlset>`;

	return new Response(xml, {
		headers: {
			'Content-Type': 'application/xml',
			'Cache-Control': 'max-age=3600, s-maxage=3600'
		}
	});
};
