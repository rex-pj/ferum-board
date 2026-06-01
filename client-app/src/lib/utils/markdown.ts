import { marked } from 'marked';
import DOMPurify from 'isomorphic-dompurify';

marked.setOptions({
	breaks: true,
	gfm: true
});

/**
 * Client-side live preview only.
 * Server-side rendering uses content_html from the backend (already sanitized with ammonia).
 */
export function renderMarkdownPreview(md: string): string {
	const html = marked.parse(md) as string;
	return DOMPurify.sanitize(html, {
		ALLOWED_TAGS: [
			'p', 'br', 'strong', 'em', 'del', 'code', 'pre', 'blockquote',
			'ul', 'ol', 'li', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
			'a', 'img', 'table', 'thead', 'tbody', 'tr', 'th', 'td'
		],
		ALLOWED_ATTR: ['href', 'src', 'alt', 'title', 'class', 'target', 'rel']
	});
}
