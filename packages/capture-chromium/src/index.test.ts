import { expect, test } from 'vitest';
import { packageName } from './index.ts';

test('exports the package name', () => {
  expect(packageName).toBe('@spokenpath/capture-chromium');
});
