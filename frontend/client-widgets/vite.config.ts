import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { resolve } from 'path';

export default defineConfig({
  plugins: [
    svelte({
      compilerOptions: {
        customElement: true,
      },
    }),
  ],
  build: {
    lib: {
      entry: resolve(__dirname, 'src/index.ts'),
      name: 'FerumWidgets',
      fileName: 'ferum-widgets',
      formats: ['iife'],
    },
    outDir: '../static/js',  // relative to frontend/client-widgets/ → frontend/static/js
    emptyOutDir: false,
    rollupOptions: {
      output: {
        entryFileNames: 'ferum-widgets.iife.js',
      },
    },
  },
});
