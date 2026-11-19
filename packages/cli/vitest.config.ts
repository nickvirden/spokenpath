import { defineConfig } from 'vitest/config';

// The command-line tests capture pages in a real browser, so they get far longer than Vitest's
// default five seconds.
export default defineConfig({
  test: {
    testTimeout: 120_000,
    hookTimeout: 120_000,
  },
});
