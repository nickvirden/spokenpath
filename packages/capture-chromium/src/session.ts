import type { Capture, Diagnostic } from '@spokenpath/snapshot-types';
import {
  type Browser,
  type BrowserContext,
  type CDPSession,
  chromium,
  errors,
  type Page,
} from 'playwright-core';
import { CaptureError } from './error.ts';
import { pageUrl, servePage } from './serve.ts';

/** How long loading, waiting for the selector and settling may each take, by default. */
export const DEFAULT_TIMEOUT_MS = 30_000;

/** The fixed conditions every page is captured under, as the snapshot records them. */
export const CONDITIONS = {
  viewport: { width: 1280, height: 720, device_pixel_ratio: 1 },
  color_scheme: 'light',
  reduced_motion: 'no-preference',
} as const satisfies Pick<Capture, 'viewport' | 'color_scheme' | 'reduced_motion'>;

/**
 * The full Chromium build in its new headless mode, rather than the separate headless shell, so
 * the tree comes from the browser users run (ADR-0008).
 */
export function launchBrowser(): Promise<Browser> {
  return chromium.launch({ channel: 'chromium', headless: true });
}

export interface PageSession {
  context: BrowserContext;
  page: Page;
  cdp: CDPSession;
  url: string;
  /** Throws the first error met while answering the page's requests. */
  throwIfServingFailed: () => void;
}

export interface OpenPageOptions {
  root: string;
  fileName: string;
  body: Uint8Array;
  diagnostics: Diagnostic[];
}

/**
 * A fresh context under the fixed conditions, serving the page, with a CDP session whose DOM and
 * Accessibility domains are enabled before the page loads.
 */
export async function openPage(browser: Browser, options: OpenPageOptions): Promise<PageSession> {
  const context = await browser.newContext({
    viewport: { width: CONDITIONS.viewport.width, height: CONDITIONS.viewport.height },
    deviceScaleFactor: CONDITIONS.viewport.device_pixel_ratio,
    colorScheme: CONDITIONS.color_scheme,
    reducedMotion: CONDITIONS.reduced_motion,
    locale: 'en-US',
    timezoneId: 'UTC',
    serviceWorkers: 'block',
    acceptDownloads: false,
  });
  const throwIfServingFailed = await servePage(context, options);
  const page = await context.newPage();
  const cdp = await context.newCDPSession(page);
  await cdp.send('DOM.enable');
  await cdp.send('Accessibility.enable');
  return { context, page, cdp, url: pageUrl(options.fileName), throwIfServingFailed };
}

/** The settle strategy's name, as `capture.settle` records it. */
export type SettleName = 'load-fonts-frames' | 'load-selector-fonts-frames';

/**
 * Loads the page and waits for it to settle: the `load` event, then the selector when one is
 * given, then `document.fonts.ready`, then two animation frames, so late content is laid out
 * before it is read.
 */
export async function loadAndSettle(
  session: PageSession,
  waitFor: string | undefined,
  timeoutMs: number,
): Promise<SettleName> {
  const { page, url } = session;
  try {
    await page.goto(url, { waitUntil: 'load', timeout: timeoutMs });
  } catch (error) {
    if (error instanceof errors.TimeoutError) {
      throw new CaptureError('timeout', `the page did not finish loading within ${timeoutMs} ms`, {
        cause: error,
      });
    }
    throw error;
  }
  session.throwIfServingFailed();
  if (waitFor !== undefined) {
    try {
      await page.waitForSelector(waitFor, { state: 'attached', timeout: timeoutMs });
    } catch (error) {
      if (error instanceof errors.TimeoutError) {
        throw new CaptureError(
          'timeout',
          `no element matched the selector ${JSON.stringify(waitFor)} within ${timeoutMs} ms`,
          { cause: error },
        );
      }
      throw error;
    }
  }
  // The race runs in the page, so a font that never loads ends the wait instead of leaving a
  // promise pending in Node after the browser closes.
  const settled = await page.evaluate(async (ms) => {
    const frames = (async () => {
      await document.fonts.ready;
      await new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done())));
      return true;
    })();
    const timeout = new Promise<boolean>((done) => setTimeout(() => done(false), ms));
    return Promise.race([frames, timeout]);
  }, timeoutMs);
  if (!settled) {
    throw new CaptureError(
      'timeout',
      `the page's fonts and animation frames did not settle within ${timeoutMs} ms`,
    );
  }
  return waitFor === undefined ? 'load-fonts-frames' : 'load-selector-fonts-frames';
}
