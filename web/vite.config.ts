import { readFileSync } from 'node:fs';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

const appVersion = readFileSync(new URL('../VERSION', import.meta.url), 'utf8').trim();

export default defineConfig({
  plugins: [sveltekit()],
  define: {
    __APP_VERSION__: JSON.stringify(appVersion)
  },
  test: {
    include: ['src/**/*.test.ts']
  }
});