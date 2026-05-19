import { defineConfig } from 'vite';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import tailwindcss from '@tailwindcss/vite';
import { fileURLToPath, URL } from 'node:url';

export default defineConfig({
  base: '/autoparq/',
  plugins: [wasm(), topLevelAwait(), tailwindcss()],
  build: {
    target: 'es2022',
    assetsInlineLimit: 0,
  },
  resolve: {
    alias: { '@wasm': fileURLToPath(new URL('./pkg', import.meta.url)) },
  },
});
