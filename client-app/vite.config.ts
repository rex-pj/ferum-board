import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

const BACKEND = process.env.API_URL ?? 'http://localhost:8080';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		fs: {
			allow: ['..']
		},
		proxy: {
			'/api': { target: BACKEND, changeOrigin: true },
			'/uploads': { target: BACKEND, changeOrigin: true },
			'/files': { target: BACKEND, changeOrigin: true }
		}
	}
});
