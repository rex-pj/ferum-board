import adapter from '@sveltejs/adapter-node';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	compilerOptions: {
		// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
		runes: ({ filename }) => (filename.split(/[/\\]/).includes('node_modules') ? undefined : true)
	},
	kit: {
		adapter: adapter(),
		// Allow file uploads up to 10 MB through SvelteKit form actions.
		// Individual backend handlers enforce their own tighter limits (logo: 2 MB, favicon: 512 KB).
		server: { bodySizeLimit: '10mb' }
	}
};

export default config;
