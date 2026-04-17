import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';

export default defineConfig({
  plugins: [svelte({ hot: false }), svelteTesting()],
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts', 'tools/**/*.test.mjs'],
    // Node 22+ exposes an experimental `localStorage` global whose methods
    // are `undefined` unless `--localstorage-file <path>` is also passed,
    // and that stub shadows jsdom's functional one. Disable it inside each
    // worker so jsdom wins. Using `execArgv` here (vs. setting NODE_OPTIONS
    // in `package.json`) keeps the flag cross-platform and leaves any
    // caller-supplied NODE_OPTIONS intact.
    execArgv: ['--no-experimental-webstorage'],
  },
});
