import { defineConfig } from 'vitest/config';

// The live golden test drives the Mac's real Safari, so it runs only through `pnpm test:safari`
// (vitest.live.config.ts). The tests here use the test-double driver and run on any platform.
export default defineConfig({
  test: {
    exclude: ['**/node_modules/**', 'src/**/*.live.test.ts'],
    testTimeout: 30_000,
    hookTimeout: 30_000,
  },
});
