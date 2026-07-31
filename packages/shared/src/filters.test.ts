import { describe, expect, test } from 'bun:test';
import {
  defaultFilters,
  filtersToQuery,
  fromStored,
  isDefault,
  pruneToType,
  toStored,
  type Filters,
} from './filters';

const base: Filters = {
  styles: [],
  dimensions: [],
  mimes: [],
  animated: true,
  static: true,
  adult: false,
  humor: true,
  epilepsy: true,
  untagged: true,
};

/**
 * The exhaustive golden table the plan calls for: all 2^4 combinations of the tag toggles.
 *
 * This is the single most error-prone rule in the product. It is written out longhand rather
 * than generated from the implementation, because a generated table would agree with a
 * mistake.
 */
describe('tag -> query inversion', () => {
  type Case = {
    untagged: boolean;
    adult: boolean;
    humor: boolean;
    epilepsy: boolean;
    nsfw: string;
    humorParam: string;
    epilepsyParam: string;
    oneoftag: string;
  };

  // untagged ON: each parameter is 'any' if that tag is wanted, 'false' if not. oneoftag empty.
  const untaggedOn: Case[] = [
    { untagged: true, adult: false, humor: false, epilepsy: false, nsfw: 'false', humorParam: 'false', epilepsyParam: 'false', oneoftag: '' },
    { untagged: true, adult: false, humor: false, epilepsy: true,  nsfw: 'false', humorParam: 'false', epilepsyParam: 'any',   oneoftag: '' },
    { untagged: true, adult: false, humor: true,  epilepsy: false, nsfw: 'false', humorParam: 'any',   epilepsyParam: 'false', oneoftag: '' },
    { untagged: true, adult: false, humor: true,  epilepsy: true,  nsfw: 'false', humorParam: 'any',   epilepsyParam: 'any',   oneoftag: '' },
    { untagged: true, adult: true,  humor: false, epilepsy: false, nsfw: 'any',   humorParam: 'false', epilepsyParam: 'false', oneoftag: '' },
    { untagged: true, adult: true,  humor: false, epilepsy: true,  nsfw: 'any',   humorParam: 'false', epilepsyParam: 'any',   oneoftag: '' },
    { untagged: true, adult: true,  humor: true,  epilepsy: false, nsfw: 'any',   humorParam: 'any',   epilepsyParam: 'false', oneoftag: '' },
    { untagged: true, adult: true,  humor: true,  epilepsy: true,  nsfw: 'any',   humorParam: 'any',   epilepsyParam: 'any',   oneoftag: '' },
  ];

  // untagged OFF: every parameter goes maximally permissive; oneoftag does the filtering.
  const untaggedOff: Case[] = [
    { untagged: false, adult: false, humor: false, epilepsy: false, nsfw: 'any', humorParam: 'any', epilepsyParam: 'any', oneoftag: '' },
    { untagged: false, adult: false, humor: false, epilepsy: true,  nsfw: 'any', humorParam: 'any', epilepsyParam: 'any', oneoftag: 'epilepsy' },
    { untagged: false, adult: false, humor: true,  epilepsy: false, nsfw: 'any', humorParam: 'any', epilepsyParam: 'any', oneoftag: 'humor' },
    { untagged: false, adult: false, humor: true,  epilepsy: true,  nsfw: 'any', humorParam: 'any', epilepsyParam: 'any', oneoftag: 'humor,epilepsy' },
    { untagged: false, adult: true,  humor: false, epilepsy: false, nsfw: 'any', humorParam: 'any', epilepsyParam: 'any', oneoftag: 'nsfw' },
    { untagged: false, adult: true,  humor: false, epilepsy: true,  nsfw: 'any', humorParam: 'any', epilepsyParam: 'any', oneoftag: 'nsfw,epilepsy' },
    { untagged: false, adult: true,  humor: true,  epilepsy: false, nsfw: 'any', humorParam: 'any', epilepsyParam: 'any', oneoftag: 'humor,nsfw' },
    { untagged: false, adult: true,  humor: true,  epilepsy: true,  nsfw: 'any', humorParam: 'any', epilepsyParam: 'any', oneoftag: 'humor,nsfw,epilepsy' },
  ];

  for (const c of [...untaggedOn, ...untaggedOff]) {
    const name = `untagged=${c.untagged} adult=${c.adult} humor=${c.humor} epilepsy=${c.epilepsy}`;
    test(name, () => {
      const q = filtersToQuery({ ...base, untagged: c.untagged, adult: c.adult, humor: c.humor, epilepsy: c.epilepsy });
      expect(q.nsfw).toBe(c.nsfw);
      expect(q.humor).toBe(c.humorParam);
      expect(q.epilepsy).toBe(c.epilepsyParam);
      expect(q.oneoftag).toBe(c.oneoftag);
    });
  }

  test('turning untagged OFF makes every tag parameter more permissive, not less', () => {
    // The counter-intuitive property, asserted directly so it survives a refactor.
    const on = filtersToQuery({ ...base, untagged: true, adult: false, humor: false, epilepsy: false });
    const off = filtersToQuery({ ...base, untagged: false, adult: false, humor: false, epilepsy: false });
    expect([on.nsfw, on.humor, on.epilepsy]).toEqual(['false', 'false', 'false']);
    expect([off.nsfw, off.humor, off.epilepsy]).toEqual(['any', 'any', 'any']);
  });
});

describe('types parameter', () => {
  test('both on', () => {
    expect(filtersToQuery({ ...base, static: true, animated: true }).types).toBe('static,animated');
  });
  test('static only', () => {
    expect(filtersToQuery({ ...base, static: true, animated: false }).types).toBe('static');
  });
  test('animated only', () => {
    expect(filtersToQuery({ ...base, static: false, animated: true }).types).toBe('animated');
  });
  test('neither is omitted rather than sent empty', () => {
    // The UI prevents this, but an empty types= would mean "match a type named empty string".
    expect(filtersToQuery({ ...base, static: false, animated: false }).types).toBeUndefined();
  });
});

describe('list parameters', () => {
  test('empty lists are omitted, not sent as empty strings', () => {
    const q = filtersToQuery(base);
    expect(q.styles).toBeUndefined();
    expect(q.dimensions).toBeUndefined();
    expect(q.mimes).toBeUndefined();
  });

  test('populated lists are comma joined', () => {
    const q = filtersToQuery({ ...base, styles: ['alternate', 'blurred'], dimensions: ['600x900'], mimes: ['image/png'] });
    expect(q.styles).toBe('alternate,blurred');
    expect(q.dimensions).toBe('600x900');
    expect(q.mimes).toBe('image/png');
  });
});

describe('defaults', () => {
  test('adult is the only tag off by default', () => {
    const d = defaultFilters('grid_p');
    expect(d.adult).toBe(false);
    expect(d.humor).toBe(true);
    expect(d.epilepsy).toBe(true);
    expect(d.untagged).toBe(true);
  });

  test('grid_p defaults to the first three dimensions', () => {
    expect(defaultFilters('grid_p').dimensions).toEqual(['600x900', '342x482', '660x930']);
  });

  test('grid_l defaults to the first two dimensions', () => {
    expect(defaultFilters('grid_l').dimensions).toEqual(['460x215', '920x430']);
  });

  test('logo has no dimension filter at all', () => {
    expect(defaultFilters('logo').dimensions).toEqual([]);
  });

  test('isDefault detects an untouched filter set', () => {
    expect(isDefault('grid_p', defaultFilters('grid_p'))).toBe(true);
  });

  test('isDefault is order insensitive', () => {
    const f = defaultFilters('grid_p');
    expect(isDefault('grid_p', { ...f, dimensions: ['660x930', '600x900', '342x482'] })).toBe(true);
  });

  test('isDefault detects any change', () => {
    const f = defaultFilters('grid_p');
    expect(isDefault('grid_p', { ...f, adult: true })).toBe(false);
    expect(isDefault('grid_p', { ...f, styles: ['blurred'] })).toBe(false);
    expect(isDefault('grid_p', { ...f, gameIdOverride: 1234 })).toBe(false);
  });
});

describe('pruneToType', () => {
  test('drops selections the target type does not offer', () => {
    // white_logo and 600x900 are grid-only; switching to hero must not carry them over.
    const pruned = pruneToType('hero', {
      ...base,
      styles: ['alternate', 'white_logo'],
      dimensions: ['600x900', '1920x620'],
      mimes: ['image/png', 'image/vnd.microsoft.icon'],
    });
    expect(pruned.styles).toEqual(['alternate']);
    expect(pruned.dimensions).toEqual(['1920x620']);
    expect(pruned.mimes).toEqual(['image/png']);
  });

  test('leaves tag toggles alone', () => {
    const pruned = pruneToType('logo', { ...base, adult: true, untagged: false });
    expect(pruned.adult).toBe(true);
    expect(pruned.untagged).toBe(false);
  });
});

describe('stored <-> working filter conversion', () => {
  test('an absent stored entry yields the defaults for that type', () => {
    // `undefined` is the ordinary case: the user has never touched this tab. Rust deliberately
    // stores nothing for it, so the defaults have one implementation and it is this one.
    expect(fromStored('grid_p', undefined)).toEqual(defaultFilters('grid_p'));
    expect(fromStored('logo', undefined)).toEqual(defaultFilters('logo'));
  });

  test('a stored entry round-trips', () => {
    const filters: Filters = {
      ...base,
      styles: ['alternate'],
      dimensions: ['600x900'],
      mimes: ['image/png'],
      adult: true,
      untagged: false,
    };
    expect(fromStored('grid_p', toStored(filters))).toEqual(filters);
  });

  test('toStored drops gameIdOverride so Reset Filters does not stick on', () => {
    // The subtle one. `gameIdOverride` is per-game and lives in Settings.game_overrides; if it
    // round-tripped through the per-tab filter state, `isDefault` would be false forever for any
    // game with an override and "Reset Filters" would never go away.
    const withOverride: Filters = { ...defaultFilters('grid_p'), gameIdOverride: 17830 };
    expect(isDefault('grid_p', withOverride)).toBe(false);

    const restored = fromStored('grid_p', toStored(withOverride));
    expect(restored.gameIdOverride).toBeUndefined();
    expect(isDefault('grid_p', restored)).toBe(true);
  });

  test('a stored dimension the type no longer offers is dropped', () => {
    // pruneToType runs on the way back in, so a value left over from an older build (or from a
    // different tab) cannot reach the API and 400 it.
    const stored = { ...toStored(defaultFilters('hero')), dimensions: ['600x900', '1920x620'] };
    expect(fromStored('hero', stored).dimensions).toEqual(['1920x620']);
  });
});
