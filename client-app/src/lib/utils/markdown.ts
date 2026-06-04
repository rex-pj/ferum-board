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
	// Linkify @username before markdown parsing so they render as links.
	const withMentions = md.replace(
		/(?<!["\w])@([a-zA-Z0-9_]{3,32})(?=[\s,.!?)]|$)/g,
		'[@$1](/u/$1)'
	);
	const html = marked.parse(withMentions) as string;
	return DOMPurify.sanitize(html, {
		ALLOWED_TAGS: [
			'p', 'br', 'strong', 'em', 'del', 'code', 'pre', 'blockquote',
			'ul', 'ol', 'li', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
			'a', 'img', 'table', 'thead', 'tbody', 'tr', 'th', 'td'
		],
		ALLOWED_ATTR: ['href', 'src', 'alt', 'title', 'class', 'target', 'rel']
	});
}
