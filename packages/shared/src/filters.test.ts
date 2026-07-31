import { describe, expect, test } from 'bun:test';
import {
  defaultFilters,
  filtersToQuery,
  fromStored,
  isDefault,
  pruneToType,
  queryFor,
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
    const d = defaultFilters();
    expect(d.adult).toBe(false);
    expect(d.humor).toBe(true);
    expect(d.epilepsy).toBe(true);
    expect(d.untagged).toBe(true);
  });

  test('the shared defaults clamp to exactly the default sizes each type used to get', () => {
    // 🔴 The property that lets ONE filter set replace five. The stored defaults are the union
    // of every type's defaults; pruning that union per type has to reproduce what each tab used
    // to get on its own, or switching to a shared set silently changes what every tab shows.
    const shared = defaultFilters();

    expect(pruneToType('grid_p', shared).dimensions).toEqual(['600x900', '342x482', '660x930']);
    expect(pruneToType('grid_l', shared).dimensions).toEqual(['460x215', '920x430']);
    expect(pruneToType('hero', shared).dimensions).toEqual(['1920x620', '3840x1240', '1600x650']);
    expect(pruneToType('logo', shared).dimensions).toEqual([]);
    expect(pruneToType('icon', shared).dimensions).toEqual([]);
  });

  test('the union carries sizes from more than one type', () => {
    // Premise for the test above: if the union somehow held only one tab's sizes, every
    // assertion there could still pass for the wrong reason.
    const shared = defaultFilters();
    expect(shared.dimensions).toContain('600x900');
    expect(shared.dimensions).toContain('460x215');
    expect(shared.dimensions).toContain('1920x620');
  });

  test('the opt-in square sizes are off by default', () => {
    // Valid for grids but they match little and are not the shape Steam renders.
    expect(defaultFilters().dimensions).not.toContain('512x512');
    expect(defaultFilters().dimensions).not.toContain('1024x1024');
  });

  test('isDefault detects an untouched filter set', () => {
    expect(isDefault(defaultFilters())).toBe(true);
  });

  test('isDefault is order insensitive', () => {
    const f = defaultFilters();
    expect(isDefault({ ...f, dimensions: [...f.dimensions].reverse() })).toBe(true);
  });

  test('isDefault detects any change', () => {
    const f = defaultFilters();
    expect(isDefault({ ...f, adult: true })).toBe(false);
    expect(isDefault({ ...f, styles: ['blurred'] })).toBe(false);
    expect(isDefault({ ...f, dimensions: [] })).toBe(false);
  });
});

describe('pruneToType', () => {
  test('drops selections the target type does not offer', () => {
    // white_logo and 600x900 are grid-only; querying hero must not carry them over.
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

  test('does not mutate the shared set it was given', () => {
    // 🔴 Load-bearing for a shared filter set: pruning is a *view* for one query. If it edited
    // in place, visiting the Logo tab once — which offers no sizes at all — would wipe every
    // size the user had chosen, for every tab, permanently.
    const shared: Filters = { ...base, dimensions: ['600x900', '1920x620'], styles: ['blurred'] };
    const before = JSON.stringify(shared);
    pruneToType('logo', shared);
    expect(JSON.stringify(shared)).toBe(before);
  });
});

describe('queryFor', () => {
  test('clamps to the tab before translating', () => {
    // 🔴 The regression this replaced a whole per-type state machine to prevent. grid_p and
    // grid_l are the SAME endpoint separated only by `dimensions`, so a query built for the
    // Wide Capsule tab while carrying the Capsule tab's 600x900 does not fail — it returns
    // portrait art, in the wide tab. That shipped once.
    const shared = defaultFilters();

    expect(queryFor('grid_l', shared).dimensions).toBe('460x215,920x430');
    expect(queryFor('grid_p', shared).dimensions).toBe('600x900,342x482,660x930');
    // Neither tab may see the other's sizes.
    expect(queryFor('grid_l', shared).dimensions).not.toContain('600x900');
    expect(queryFor('grid_p', shared).dimensions).not.toContain('460x215');
  });

  test('omits dimensions entirely for the endpoints that reject them', () => {
    // logos and icons 400 on any `dimensions` value at all.
    expect(queryFor('logo', defaultFilters()).dimensions).toBeUndefined();
    expect(queryFor('icon', defaultFilters()).dimensions).toBeUndefined();
  });

  test('a style valid only for another endpoint is dropped rather than sent', () => {
    // The shared set may hold `white_logo` from the Capsule tab; the logos endpoint has its own
    // vocabulary and would reject it with a 400 that reads as a service failure.
    const shared: Filters = { ...base, styles: ['white_logo', 'official'] };
    expect(queryFor('logo', shared).styles).toBe('official');
    expect(queryFor('grid_p', shared).styles).toBe('white_logo');
  });

  test('the tag toggles are shared verbatim across every tab', () => {
    // The whole point of one filter set: content preferences are not per-tab.
    const shared: Filters = { ...base, adult: true, untagged: true };
    for (const type of ['grid_p', 'grid_l', 'hero', 'logo', 'icon'] as const) {
      expect(queryFor(type, shared).nsfw).toBe('any');
    }
  });
});

describe('stored <-> working filter conversion', () => {
  test('nothing stored yields the defaults', () => {
    // The ordinary first-run case. Rust stores `null`, and the defaults are filled in here so
    // they have exactly one implementation.
    expect(fromStored(undefined)).toEqual(defaultFilters());
    expect(fromStored(null)).toEqual(defaultFilters());
  });

  test('a stored set round-trips unchanged', () => {
    const filters: Filters = {
      ...base,
      styles: ['alternate'],
      dimensions: ['600x900', '1920x620'],
      mimes: ['image/png'],
      adult: true,
      untagged: false,
    };
    expect(fromStored(toStored(filters))).toEqual(filters);
  });

  test('loading does NOT prune, so sizes belonging to other tabs survive a round trip', () => {
    // 🔴 The counterpart to pruning at query time. If load pruned, opening the Logo tab and
    // saving would drop every size the user had picked — they would come back to the Capsule
    // tab and find their selection silently reset.
    const shared: Filters = { ...base, dimensions: ['600x900', '460x215', '1920x620'] };
    expect(fromStored(toStored(shared)).dimensions).toEqual([
      '600x900',
      '460x215',
      '1920x620',
    ]);
  });
});
