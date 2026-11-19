import { readFileSync } from 'node:fs';
import type { Snapshot } from '@spokenpath/snapshot-types';
import { expect, test } from 'vitest';
import { normaliseSafariForGolden } from './normalise.ts';

const basic = JSON.parse(
  readFileSync(new URL('../../../fixtures/snapshots/safari-webdriver-basic.json', import.meta.url), 'utf8'),
) as Snapshot;

test('sets the run, port and display fields and keeps everything else', () => {
  const captured: Snapshot = structuredClone(basic);
  captured.volatile.captured_at = '2026-11-19T10:00:00.123Z';
  captured.producer.version = '0.6.0';
  captured.capture.url = 'http://127.0.0.1:53124/basic.html';
  captured.capture.viewport = { width: 1280, height: 664, device_pixel_ratio: 2 };
  captured.capture.color_scheme = 'dark';
  captured.capture.reduced_motion = 'reduce';
  const normalised = normaliseSafariForGolden(captured);
  expect(normalised.volatile.captured_at).toBe('1970-01-01T00:00:00Z');
  expect(normalised.producer.version).toBe('0.0.0');
  expect(normalised.capture).toEqual({
    ...captured.capture,
    url: 'http://127.0.0.1:0/basic.html',
    viewport: { width: 1280, height: 720, device_pixel_ratio: 1 },
    color_scheme: 'light',
    reduced_motion: 'no-preference',
  });
  expect(normalised.payload).toEqual(captured.payload);
  expect(normalised.spans).toEqual(captured.spans);
  expect(captured.capture.url).toBe('http://127.0.0.1:53124/basic.html');
});
