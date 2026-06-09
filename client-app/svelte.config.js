import adapter from '@sveltejs/adapter-node';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	compilerOptions: {
		// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
		runes: ({ filename }) => (filename.split(/[/\\]/).includes('node_modules') ? undefined : true)
	},
	kit: {
		// bodySizeLimit covers plugin .fpkg uploads (max 50 MB) and logo/favicon uploads.
		// Individual backend handlers enforce their own tighter limits.
		adapter: adapter({ bodySizeLimit: '52mb' })
	}
};

export default config;
