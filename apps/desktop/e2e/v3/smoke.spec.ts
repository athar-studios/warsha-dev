import { expect, test } from '@playwright/test';

import { openV3 } from './support';

/**
 * Local route list for v3 smoke tests.
 *
 * Source of truth: apps/desktop/src/v3/router/routes.ts.
 * We maintain this local list to avoid importing the app's component graph into
 * Playwright's Node runner (which causes CJS/ESM module issues with @warsha/i18n).
 * Keep in sync with routes.ts by review.
 */
interface SmokeRoute {
  path: string;
  sampleUrl: string;
}

const ROUTES: readonly SmokeRoute[] = [
  { path: '/onboarding', sampleUrl: '/onboarding' },
  { path: '/', sampleUrl: '/' },
  { path: '/projects', sampleUrl: '/projects' },
  { path: '/p/:projectId', sampleUrl: '/p/proj-1' },
  { path: '/p/:projectId/r/:runId/:tab', sampleUrl: '/p/proj-1/r/run-1/overview' },
  { path: '/inbox', sampleUrl: '/inbox' },
  { path: '/settings/:section', sampleUrl: '/settings/general' },
];

const LOCALES = ['ar', 'en'] as const;

for (const locale of LOCALES) {
  for (const route of ROUTES) {
    test(`route ${route.path} boots in ${locale} with main[data-ready="true"]`, async ({
      page,
    }) => {
      await openV3(page, route.sampleUrl, { lang: locale });
      const main = page.locator('main[data-ready="true"]');
      await expect(main).toBeVisible();
    });
  }
}

test('v3 loads bundled IBM Plex Sans Arabic and IBM Plex Mono offline with non-loopback requests aborted', async ({
  page,
}) => {
  const blockedExternal: string[] = [];
  await page.route(/^https?:\/\/(?!(?:localhost|127\.0\.0\.1|\[::1\])(?::\d+)?(?:\/|$))/i, (route) => {
    blockedExternal.push(route.request().url());
    return route.abort();
  });

  await openV3(page, '/p/proj-ui/r/run-code/overview', { lang: 'ar', mode: 'advanced' });
  await page.evaluate(async () => {
    await document.fonts.ready;
  });

  const loadedFamilies = await page.evaluate(() => {
    const out: { family: string; status: string }[] = [];
    for (const face of document.fonts) {
      if (face.status === 'loaded') {
        out.push({ family: face.family.replace(/["']/g, ''), status: face.status });
      }
    }
    return out;
  });

  expect(blockedExternal).toEqual([]);
  expect(loadedFamilies.some((f) => f.family.includes('IBM Plex Sans Arabic'))).toBe(true);
  expect(loadedFamilies.some((f) => f.family.includes('IBM Plex Mono'))).toBe(true);
});

