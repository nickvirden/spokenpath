import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { SOURCE_ATTRIBUTE } from '@spokenpath/instrument-html';
import type { Snapshot } from '@spokenpath/snapshot-types';

/** The directory holding the capture fixtures, relative to which test pages are named. */
export const pagesUrl = new URL('../../../fixtures/pages/', import.meta.url);

/** The directory holding the committed capture goldens. */
export const goldensUrl = new URL('../../../fixtures/snapshots/chromium/', import.meta.url);

/** The `*.html` files directly in `directory` (relative to `fixtures/pages/`, `''` or ending in `/`). */
export function htmlPagesIn(directory: string): string[] {
  return readdirSync(new URL(directory, pagesUrl), { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith('.html'))
    .map((entry) => `${directory}${entry.name}`)
    .sort();
}

/**
 * The pages that have committed goldens: the top level of `fixtures/pages/`, `focus/`, the
 * name-provenance, set-position and exclusion pages that the Rust model tests read, the
 * `reading-order/` and `navigation/` pages that the Rust announce tests read, and the
 * `cross-check/` pages where Safari's roles and names are expected to differ from Chromium's.
 */
export const capturePages: string[] = [
  ...htmlPagesIn(''),
  ...htmlPagesIn('focus/'),
  ...htmlPagesIn('names/'),
  ...htmlPagesIn('set-position/'),
  ...htmlPagesIn('excluded/'),
  ...htmlPagesIn('reading-order/'),
  ...htmlPagesIn('navigation/'),
  ...htmlPagesIn('cross-check/'),
];

/** The file system path of a page named relative to `fixtures/pages/`. */
export function pagePath(page: string): string {
  return fileURLToPath(new URL(page, pagesUrl));
}

export function pageBytes(page: string): Buffer {
  return readFileSync(new URL(page, pagesUrl));
}

/** The Chromium payload of a snapshot, or a thrown error naming the engine it has instead. */
export function chromiumPayload(snapshot: Snapshot) {
  if (snapshot.payload.engine !== 'chromium') {
    throw new Error(`the snapshot's payload is for ${snapshot.payload.engine}, not chromium`);
  }
  return snapshot.payload;
}

/** The value of the `id` attribute of a DOM node. */
export function elementId(snapshot: Snapshot, node: number | undefined): string | undefined {
  if (node === undefined) {
    return undefined;
  }
  return chromiumPayload(snapshot).dom[node]?.attributes.find((attribute) => attribute.name === 'id')?.value;
}

/**
 * The problems with how a snapshot carries source locations: a DOM attribute named
 * `SOURCE_ATTRIBUTE`, or an element of origin `source` whose span's start tag, sliced from the
 * page's bytes, does not begin with `<` and the element's tag name.
 */
export function sourceTaggingProblems(snapshot: Snapshot, bytes: Buffer): string[] {
  const problems: string[] = [];
  for (const node of chromiumPayload(snapshot).dom) {
    if (node.attributes.some((attribute) => attribute.name === SOURCE_ATTRIBUTE)) {
      problems.push(`node ${node.id} keeps the ${SOURCE_ATTRIBUTE} attribute`);
    }
    if (node.origin !== 'source') {
      continue;
    }
    const span = node.span === undefined ? undefined : snapshot.spans[node.span];
    if (span === undefined || span.id !== node.span) {
      problems.push(`node ${node.id} has origin source but no span`);
      continue;
    }
    const startTag = bytes.subarray(span.start_tag.start, span.start_tag.end).toString('utf8');
    if (!startTag.toLowerCase().startsWith(`<${node.tag?.toLowerCase()}`)) {
      problems.push(
        `node ${node.id} (<${node.tag}>) has span ${span.id}, which starts ${JSON.stringify(startTag)}`,
      );
    }
  }
  return problems;
}
