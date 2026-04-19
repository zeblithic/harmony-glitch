import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';

export default defineConfig({
  plugins: [svelte({ hot: false }), svelteTesting()],
  // Vite server FS sandbox is relaxed so vitest can resolve
  // `node_modules` when it is symlinked into a git worktree (common dev
  // setup for parallel branches). This affects only the local vitest
  // dev-server, never a production bundle. Narrow this to `server.fs.allow`
  // paths if tooling ever runs untrusted code here.
  server: {
    fs: {
      strict: false,
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['@testing-library/jest-dom/vitest'],
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
