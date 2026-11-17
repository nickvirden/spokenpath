import { expect, test } from 'vitest';
import { packageName } from './index.js';

test('exports the package name', () => {
  expect(packageName).toBe('@spokenpath/instrument-html');
});
