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
  isVideoPreview,
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
