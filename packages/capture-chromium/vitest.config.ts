import { defineConfig } from 'vitest/config';

// Capture tests launch a real browser, often several per test, so they get far longer than
// Vitest's default five seconds.
export default defineConfig({
  test: {
    testTimeout: 120_000,
    hookTimeout: 120_000,
  },
});
