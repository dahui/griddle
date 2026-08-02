/**
 * The filter vocabularies, held to the same fixture the Rust side asserts against.
 *
 * These tables exist in two languages: the UI offers values from here, and Rust's
 * `AssetQuery::from_params` validates against its own copy. A value present on one side and
 * missing on the other is refused locally with an error naming a checkbox the UI itself
 * rendered — and the user just sees a filter that returns nothing.
 *
 * Same pattern as `logo.test.ts`, for the same reason: the fixture is the thing that stops them
 * drifting, and neither language owns it.
 */
import { describe, expect, test } from 'bun:test';
import vocabulary from '../fixtures/filter-vocabulary.json';
import {
  ASSET_TYPES,
  DIMENSIONS,
  MIMES,
  STYLE_LABEL,
  STYLES,
  ZOOM,
  ZOOM_TARGETS,
  assetPageUrl,
  isVideoPreview,
  zoomFor,
  zoomStep,
  type AssetType,
} from './assets';
import { PAGE_LIMIT } from './filters';
import { STEAM_CDN_BASENAME, steamCdnUrl, steamIconUrl } from './steamArt';

const TYPES = ASSET_TYPES as readonly AssetType[];

describe('filter vocabulary matches the shared fixture', () => {
  test('the fixture really loaded', () => {
    // Premise. Without this, an empty or renamed fixture would make every `for` loop below
    // iterate zero times and the whole suite would pass while asserting nothing.
    expect(Object.keys(vocabulary.dimensions)).toHaveLength(5);
    expect(Object.keys(vocabulary.styles)).toHaveLength(5);
    expect(Object.keys(vocabulary.mimes)).toHaveLength(5);
  });

  for (const type of TYPES) {
    test(`dimensions for ${type}`, () => {
      expect(DIMENSIONS[type].all).toEqual(vocabulary.dimensions[type].all);
      expect(DIMENSIONS[type].default).toEqual(vocabulary.dimensions[type].default);
    });

    test(`styles for ${type}`, () => {
      expect(STYLES[type]).toEqual(vocabulary.styles[type]);
    });

    test(`mimes for ${type}`, () => {
      expect(MIMES[type]).toEqual(vocabulary.mimes[type]);
    });
  }

  test('page limit', () => {
    expect(PAGE_LIMIT).toBe(vocabulary.pageLimit);
  });

  test('every default dimension is also offered', () => {
    // A default that is not in `all` cannot be unticked in the UI and cannot be re-ticked once
    // removed, which reads as a filter that will not turn off.
    for (const type of TYPES) {
      for (const value of DIMENSIONS[type].default) {
        expect(DIMENSIONS[type].all).toContain(value);
      }
    }
  });

  test('every style has a display label', () => {
    // A missing label renders the raw API value ("white_logo") in the filter panel.
    for (const type of TYPES) {
      for (const style of STYLES[type]) {
        expect(STYLE_LABEL[style]).toBeTruthy();
      }
    }
  });
});

describe('animated previews', () => {
  test('a .webm thumbnail is a video, not an image', () => {
    // SteamGridDB serves animated artwork with a `.webm` *thumbnail*. Rendering that in an
    // <img> produces a broken-image icon, which is indistinguishable from missing artwork —
    // 12% of Cyberpunk 2077's capsules looked broken because of exactly this.
    expect(isVideoPreview('https://cdn2.steamgriddb.com/thumb/51f993d2.webm')).toBe(true);
    expect(isVideoPreview('https://cdn2.steamgriddb.com/thumb/f39b7817.jpg')).toBe(false);
    expect(isVideoPreview('https://cdn2.steamgriddb.com/grid/51f993d2.webp')).toBe(false);
    expect(isVideoPreview('https://cdn2.steamgriddb.com/grid/abc.png')).toBe(false);
  });

  test('the extension is read from the path, not the whole URL', () => {
    // A query string must neither defeat the check nor fake it.
    expect(isVideoPreview('https://cdn2.steamgriddb.com/thumb/a.webm?v=2')).toBe(true);
    expect(isVideoPreview('https://cdn2.steamgriddb.com/thumb/a.jpg?x=.webm')).toBe(false);
    expect(isVideoPreview('https://cdn2.steamgriddb.com/thumb/a.webm#frag')).toBe(true);
  });

  test('a missing thumbnail is not a video', () => {
    expect(isVideoPreview(null)).toBe(false);
    expect(isVideoPreview(undefined)).toBe(false);
    expect(isVideoPreview('')).toBe(false);
  });

  test('webp is NOT the signal — an APNG is animated too', () => {
    // The check that looks obviously right and is wrong. Of the 23 webm-thumbed capsules
    // measured on Cyberpunk, 7 report `mime: image/png` because they are APNGs. Keying off the
    // mime would leave a third of them rendering as broken images.
    const apng = { mime: 'image/png', thumb: 'https://cdn2.steamgriddb.com/thumb/x.webm' };
    expect(apng.mime).toBe('image/png');
    expect(isVideoPreview(apng.thumb)).toBe(true);
  });
});

describe('tile zoom', () => {
  test('an unset, absurd or corrupt value falls back to the default', () => {
    // `settings.json` is a file a user can edit, and a zero or a NaN here is a grid with no
    // columns at all — an empty browsing tab that looks like "SteamGridDB has nothing".
    for (const type of ZOOM_TARGETS) {
      expect(zoomFor(type, {})).toBe(ZOOM[type].default);
      expect(zoomFor(type, { [type]: Number.NaN })).toBe(ZOOM[type].default);
      expect(zoomFor(type, { [type]: Number.POSITIVE_INFINITY })).toBe(ZOOM[type].default);
      expect(zoomFor(type, { [type]: 'big' as unknown as number })).toBe(ZOOM[type].default);
    }
  });

  test('a stored value outside the range is clamped, not discarded', () => {
    // Clamped on read, so a build that narrows the bounds does not silently rewrite a choice
    // the user made under the old ones. Same reasoning as clamping filters at query time.
    expect(zoomFor('grid_p', { grid_p: 1000 })).toBe(ZOOM.grid_p.max);
    expect(zoomFor('grid_p', { grid_p: -5 })).toBe(ZOOM.grid_p.min);
    expect(zoomFor('grid_p', { grid_p: 12 })).toBe(12);
  });

  test('stepping stops at the bounds instead of running past them', () => {
    expect(zoomStep('grid_p', ZOOM.grid_p.max, 1)).toBe(ZOOM.grid_p.max);
    expect(zoomStep('grid_p', ZOOM.grid_p.min, -1)).toBe(ZOOM.grid_p.min);
    expect(zoomStep('grid_p', 10, 1)).toBe(11);
    expect(zoomStep('grid_p', 10, -1)).toBe(9);
  });

  test('a fractional step lands exactly on the bound', () => {
    // 1.5-rem steps off a 9.5 default accumulate float error, and the +/- buttons are disabled by
    // comparing against the bound for equality — so 31.999999999999996 leaves "bigger" live
    // forever at the top of the range.
    let value = ZOOM.grid_l.default;
    for (let i = 0; i < 40; i++) value = zoomStep('grid_l', value, 1);
    expect(value).toBe(ZOOM.grid_l.max);

    for (let i = 0; i < 80; i++) value = zoomStep('grid_l', value, -1);
    expect(value).toBe(ZOOM.grid_l.min);
  });

  test('every target has a usable range with the default inside it', () => {
    // Premise: the two non-asset-type grids are in here. Without this the loop would still pass
    // if `ZOOM_TARGETS` silently lost them, and the library would go back to being the one grid
    // people scroll most and cannot resize.
    expect(ZOOM_TARGETS).toContain('library');
    expect(ZOOM_TARGETS).toContain('current');
    expect(ZOOM_TARGETS).toHaveLength(TYPES.length + 2);

    for (const type of ZOOM_TARGETS) {
      const { min, max, default: dflt, step } = ZOOM[type];
      expect(min).toBeLessThan(max);
      expect(dflt).toBeGreaterThanOrEqual(min);
      expect(dflt).toBeLessThanOrEqual(max);
      expect(step).toBeGreaterThan(0);
    }
  });
});

describe('the SteamGridDB page for an asset', () => {
  test('builds the measured route shapes', () => {
    // Each of these was fetched with a browser User-Agent and returned 200 with a title naming
    // the game and the author. The site 403s a bare client, so a plain probe proves nothing.
    expect(assetPageUrl('hero', 100)).toBe('https://www.steamgriddb.com/hero/100');
    expect(assetPageUrl('logo', 1)).toBe('https://www.steamgriddb.com/logo/1');
    expect(assetPageUrl('icon', 1)).toBe('https://www.steamgriddb.com/icon/1');
  });

  test('both capsule types collapse to /grid/, because there is no /grid_p/ route', () => {
    // The same collapsing as the API, where `grids` serves both and only `dimensions` separates
    // them. Inventing a per-type segment here would 404 on the two most-used tabs.
    expect(assetPageUrl('grid_p', 1)).toBe('https://www.steamgriddb.com/grid/1');
    expect(assetPageUrl('grid_l', 1)).toBe('https://www.steamgriddb.com/grid/1');
  });

  test('every asset type produces a link the browser allowlist accepts', () => {
    // `browser::open` refuses anything that is not https on steamgriddb.com or a subdomain, and
    // a refusal surfaces as an error toast rather than as a dead link — so a new asset type
    // added without a segment must fail here, not in front of the user.
    for (const type of TYPES) {
      const url = assetPageUrl(type, 7);
      expect(url.startsWith('https://www.steamgriddb.com/')).toBe(true);
      expect(url.endsWith('/7')).toBe(true);
      expect(url).not.toContain('undefined');
    }
  });
});

describe('steam CDN artwork', () => {
  test('builds the measured URL shape', () => {
    expect(steamCdnUrl(620, 'grid_p')).toBe(
      'https://shared.steamstatic.com/store_item_assets/steam/apps/620/library_600x900.jpg',
    );
    expect(steamCdnUrl(620, 'grid_l')).toBe(
      'https://shared.steamstatic.com/store_item_assets/steam/apps/620/header.jpg',
    );
  });

  test('the capsule uses the CDN name, never the on-disk name', () => {
    // The trap this table exists to prevent. 1030300 stores its capsule on disk as
    // `<sha1>/library_capsule.jpg`, but the CDN serves it as `library_600x900.jpg` — and
    // `library_capsule.jpg` is a measured 404 on that host for every app.
    expect(STEAM_CDN_BASENAME.grid_p).toBe('library_600x900.jpg');
    expect(Object.values(STEAM_CDN_BASENAME)).not.toContain('library_capsule.jpg');
    expect(Object.values(STEAM_CDN_BASENAME)).not.toContain('library_header.jpg');
  });

  test('icons have no fixed name, so they return null', () => {
    // They are content-hashed and live on a different host entirely. Returning a plausible URL
    // here would 404 on every app.
    expect(steamCdnUrl(620, 'icon')).toBeNull();
    expect(steamIconUrl(620, 'abc123')).toBe(
      'https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/620/abc123.jpg',
    );
  });

  test('every non-icon asset type has a CDN name', () => {
    for (const type of TYPES) {
      if (type === 'icon') continue;
      expect(STEAM_CDN_BASENAME[type]).toBeTruthy();
    }
  });
});
