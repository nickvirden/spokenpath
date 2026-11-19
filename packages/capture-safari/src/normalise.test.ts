import { readFileSync } from 'node:fs';
import type { Snapshot } from '@spokenpath/snapshot-types';
import { describe, expect, test } from 'vitest';
import { normaliseSafariAxForGolden, normaliseSafariForGolden } from './normalise.ts';

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

const axBasic = JSON.parse(
  readFileSync(new URL('../../../fixtures/snapshots/safari-ax-basic.json', import.meta.url), 'utf8'),
) as Snapshot;

describe('normaliseSafariAxForGolden', () => {
  const nonce = '0123456789abcdef0123456789abcdef';
  const served = `http://127.0.0.1:53124/basic.html?spokenpath-nonce=${nonce}`;
  /** The fixture as a capture on port 53124 with a nonce and the Mac's display values. */
  function captured(): Snapshot {
    const snapshot: Snapshot = structuredClone(axBasic);
    snapshot.capture.url = served;
    snapshot.capture.color_scheme = 'dark';
    if (snapshot.payload.engine !== 'safari-ax') {
      throw new Error('a Safari AX payload');
    }
    const [webArea, heading] = snapshot.payload.nodes ?? [];
    if (webArea === undefined || heading === undefined) {
      throw new Error('the fixture has two nodes');
    }
    webArea.attributes.AXURL = { type: 'url', value: served };
    webArea.attributes.AXDescription = {
      type: 'string',
      value: `see http://127.0.0.1:53124/help.html#${nonce}`,
    };
    webArea.attributes.AXLinked = { type: 'array', items: [{ type: 'url', value: served }] };
    heading.attributes.AXFrame = { type: 'rect', x: 8, y: 108.5, width: 1264, height: 37 };
    heading.attributes.AXVisibleCharacterRange = { type: 'range', location: 2, length: 12 };
    return snapshot;
  }

  test('sets the port and nonce in the URL and in every string, and zeroes the display values', () => {
    const normalised = normaliseSafariAxForGolden(captured());
    expect(normalised.capture.url).toBe('http://127.0.0.1:0/basic.html?spokenpath-nonce=0');
    expect(normalised.capture.color_scheme).toBe('light');
    if (normalised.payload.engine !== 'safari-ax') {
      throw new Error('a Safari AX payload');
    }
    const [webArea, heading] = normalised.payload.nodes ?? [];
    expect(webArea?.attributes.AXURL).toEqual({
      type: 'url',
      value: 'http://127.0.0.1:0/basic.html?spokenpath-nonce=0',
    });
    expect(webArea?.attributes.AXDescription).toEqual({
      type: 'string',
      value: 'see http://127.0.0.1:0/help.html#0',
    });
    expect(webArea?.attributes.AXLinked).toEqual({
      type: 'array',
      items: [{ type: 'url', value: 'http://127.0.0.1:0/basic.html?spokenpath-nonce=0' }],
    });
    expect(webArea?.attributes._AXPrimaryScreenHeight).toEqual({ type: 'number', value: 0 });
    expect(webArea?.attributes.AXLayoutCount).toEqual({ type: 'number', value: 0 });
    expect(heading?.attributes.AXFrame).toEqual({ type: 'rect', x: 0, y: 0, width: 0, height: 0 });
    expect(heading?.attributes.AXPosition).toEqual({ type: 'point', x: 0, y: 0 });
    expect(heading?.attributes.AXSize).toEqual({ type: 'size', width: 0, height: 0 });
    expect(heading?.attributes.AXVisibleCharacterRange).toEqual({ type: 'range', location: 0, length: 0 });
  });

  test('keeps every other value, and an error where a display value would be', () => {
    const before = captured();
    if (before.payload.engine !== 'safari-ax') {
      throw new Error('a Safari AX payload');
    }
    const text = before.payload.nodes?.[2];
    if (text === undefined) {
      throw new Error('the fixture has a node 2');
    }
    text.attributes.AXSize = { type: 'error', code: -25212 };
    const normalised = normaliseSafariAxForGolden(before);
    if (normalised.payload.engine !== 'safari-ax') {
      throw new Error('a Safari AX payload');
    }
    const after = normalised.payload.nodes?.[2];
    expect(after?.attributes.AXSize).toEqual({ type: 'error', code: -25212 });
    expect(after?.attributes.AXValue).toEqual(text.attributes.AXValue);
    expect(after?.attributes.AXParent).toEqual(text.attributes.AXParent);
    expect(normalised.spans).toEqual(before.spans);
    expect(before.capture.url).toBe(served);
  });
});
