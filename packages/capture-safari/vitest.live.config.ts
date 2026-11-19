import { defineConfig } from 'vitest/config';

// Safari allows one automation session at a time, so the live pages are captured one after
// another in a single worker, and each capture launches Safari, so the limits are long.
export default defineConfig({
  test: {
    include: ['src/**/*.live.test.ts'],
    fileParallelism: false,
    maxWorkers: 1,
    testTimeout: 120_000,
    hookTimeout: 120_000,
  },
});
