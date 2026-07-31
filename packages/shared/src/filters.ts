/**
 * Filter state -> SteamGridDB query parameters.
 *
 * # The one rule worth reading carefully
 *
 * SteamGridDB has no "untagged" parameter. The Decky plugin synthesizes one by *inverting*
 * the meaning of the three tag parameters, and getting this backwards produces a filter that
 * looks plausible and silently returns the wrong set:
 *
 * - **`untagged` on** — the tag toggles are *exclusions*. Each parameter is `'any'` when the
 *   user wants that tag included and `'false'` when they don't, and `oneoftag` is empty. So
 *   untagged assets come back, plus whichever tagged ones were left enabled.
 *
 * - **`untagged` off** — the tag toggles are *requirements*. All three parameters go to
 *   `'any'` (so the tag filters stop excluding anything) and `oneoftag` carries the enabled
 *   tags, which restricts results to assets bearing at least one of them. Untagged assets are
 *   thereby excluded.
 *
 * The counter-intuitive part is that turning `untagged` *off* sets every tag parameter to its
 * most permissive value. `oneoftag` is doing all the filtering at that point.
 *
 * SteamGridDB's own docs describe `oneoftag` as: *"Filter results using any of the supplied
 * tags. Use combined with the above tag params set to `any` to mimic untagged
 * functionality."* `[VERIFIED-DOCS — openapi.yml 2.10.0]`
 */

import { ASSET_TYPES, DIMENSIONS, MIMES, STYLES, type AssetType } from './assets';

/**
 * The content filters — **one set, shared by every asset type**.
 *
 * The Decky plugin keys these per type (`filters_<type>`) and so did this, until it became clear
 * that having to re-pick "no adult content" on five separate tabs is busywork rather than a
 * feature.
 *
 * Sizes and styles *do* have per-type vocabularies, and that is handled by **clamping rather
 * than splitting**: this object holds the union, and {@link pruneToType} narrows it to whatever
 * the current tab can use at the moment the query is built. A selection another tab cannot show
 * is therefore kept rather than discarded, so switching back restores it.
 */
export interface Filters {
  styles: string[];
  dimensions: string[];
  mimes: string[];
  /** Both may not be false at once; the UI prevents it and {@link filtersToQuery} tolerates it. */
  animated: boolean;
  static: boolean;
  /** "Adult Content". Off by default — the only tag that is. */
  adult: boolean;
  humor: boolean;
  epilepsy: boolean;
  untagged: boolean;
}

/**
 * The union of a per-type vocabulary, in first-seen order.
 *
 * Derived rather than written out, so adding a style, size or format cannot leave the defaults
 * behind. Clamping a union with {@link pruneToType} reproduces each type's own list exactly —
 * which is the property that lets one shared filter set replace five without changing what any
 * tab shows out of the box.
 */
const unionOf = (per: (type: AssetType) => readonly string[]): string[] => [
  ...new Set(ASSET_TYPES.flatMap(per)),
];

const DEFAULT_STYLES: string[] = unionOf((t) => STYLES[t]);
const DEFAULT_MIMES: string[] = unionOf((t) => MIMES[t]);
const DEFAULT_DIMENSIONS: string[] = unionOf((t) => DIMENSIONS[t].default);

/**
 * The starting filter set — **matching the Decky plugin's, deliberately.**
 *
 * Decky seeds each group from its own vocabulary (`STYLES[type].default`, `MIMES[type].default`,
 * `DIMENSIONS[type].default`), so every style and format box starts *ticked* and unticking one is
 * a narrowing action. `[VERIFIED-SOURCE — decky-steamgriddb src/modals/FiltersModal.tsx]` This
 * used to start them empty, which queries identically but reads as a filter nobody has set up
 * yet: the vocabulary was invisible until you ticked something.
 *
 * The tag toggles were already identical to Decky's: everything on except Adult.
 *
 * 🔴 One deliberate divergence remains. Decky lists 39 icon dimensions; every one of them is an
 * HTTP 400 against the live `icons` endpoint, so {@link DIMENSIONS} carries none for that type.
 * A verified measurement outranks parity. `[VERIFIED-BOX 2026-07-30]`
 *
 * Note what "all ticked" costs: a style SteamGridDB *adds* later is excluded until it is added
 * here, where leaving the group empty would have included it. That is the same closed-vocabulary
 * trade this project already makes in `Dimensions`, and it is Decky's behaviour too.
 */
export function defaultFilters(): Filters {
  return {
    styles: [...DEFAULT_STYLES],
    dimensions: [...DEFAULT_DIMENSIONS],
    mimes: [...DEFAULT_MIMES],
    animated: true,
    static: true,
    adult: false,
    humor: true,
    epilepsy: true,
    untagged: true,
  };
}

/** True when `filters` still matches {@link defaultFilters} — drives "Reset Filters" visibility. */
export function isDefault(filters: Filters): boolean {
  const d = defaultFilters();
  const sameSet = (a: string[], b: string[]) =>
    a.length === b.length && [...a].sort().join(',') === [...b].sort().join(',');
  return (
    sameSet(filters.styles, d.styles) &&
    sameSet(filters.dimensions, d.dimensions) &&
    sameSet(filters.mimes, d.mimes) &&
    filters.animated === d.animated &&
    filters.static === d.static &&
    filters.adult === d.adult &&
    filters.humor === d.humor &&
    filters.epilepsy === d.epilepsy &&
    filters.untagged === d.untagged
  );
}

export type QueryParams = Record<string, string>;

/**
 * Build the SteamGridDB query string parameters for a filter state.
 *
 * Empty array filters are omitted entirely rather than sent as `""` — an empty `styles=`
 * would be interpreted as "match a style whose name is the empty string".
 */
export function filtersToQuery(filters: Filters): QueryParams {
  const params: QueryParams = {};

  if (filters.styles.length > 0) params.styles = filters.styles.join(',');
  if (filters.dimensions.length > 0) params.dimensions = filters.dimensions.join(',');
  if (filters.mimes.length > 0) params.mimes = filters.mimes.join(',');

  const types: string[] = [];
  if (filters.static) types.push('static');
  if (filters.animated) types.push('animated');
  if (types.length > 0) params.types = types.join(',');

  if (filters.untagged) {
    // Tag toggles act as exclusions; untagged assets are included.
    params.nsfw = filters.adult ? 'any' : 'false';
    params.humor = filters.humor ? 'any' : 'false';
    params.epilepsy = filters.epilepsy ? 'any' : 'false';
    params.oneoftag = '';
  } else {
    // Tag toggles act as requirements. Every tag parameter goes maximally permissive and
    // `oneoftag` does the filtering, which excludes untagged assets.
    params.nsfw = 'any';
    params.humor = 'any';
    params.epilepsy = 'any';
    const oneof: string[] = [];
    if (filters.humor) oneof.push('humor');
    if (filters.adult) oneof.push('nsfw');
    if (filters.epilepsy) oneof.push('epilepsy');
    params.oneoftag = oneof.join(',');
  }

  return params;
}

/** Values above 50 are ignored by the API, so asking for more is a wasted round trip. */
export const PAGE_LIMIT = 50;

/**
 * The stored shape, as `sgdb-core`'s `FilterState` serialises it.
 *
 * Structurally identical to {@link Filters}; kept as its own name because it is the wire
 * contract with Rust, where `static` is a keyword and therefore a `#[serde(rename)]`.
 */
export interface StoredFilters {
  untagged: boolean;
  adult: boolean;
  humor: boolean;
  epilepsy: boolean;
  styles: string[];
  dimensions: string[];
  mimes: string[];
  animated: boolean;
  static: boolean;
}

/**
 * Stored filters → working filters, falling back to the defaults.
 *
 * `null`/`undefined` means the user has never changed them. The defaults are filled in here
 * rather than in Rust so they have exactly one implementation — the one `defaultFilters` above
 * already tests. **No pruning happens here**: the stored set is the union across every type, and
 * narrowing it on load would permanently discard whatever the current tab cannot show.
 */
export function fromStored(stored: StoredFilters | null | undefined): Filters {
  if (!stored) return defaultFilters();
  return {
    styles: stored.styles,
    dimensions: stored.dimensions,
    mimes: stored.mimes,
    animated: stored.animated,
    static: stored.static,
    adult: stored.adult,
    humor: stored.humor,
    epilepsy: stored.epilepsy,
    untagged: stored.untagged,
  };
}

/** Working filters → the stored shape. */
export function toStored(filters: Filters): StoredFilters {
  return {
    untagged: filters.untagged,
    adult: filters.adult,
    humor: filters.humor,
    epilepsy: filters.epilepsy,
    styles: filters.styles,
    dimensions: filters.dimensions,
    mimes: filters.mimes,
    animated: filters.animated,
    static: filters.static,
  };
}

/**
 * Clamp a filter set to the options one asset type actually offers.
 *
 * 🔴 **This is what keeps a shared filter set honest, and it must run at query time — every
 * time — against the tab being queried.** `grid_p` and `grid_l` are the *same* SteamGridDB
 * endpoint separated only by `dimensions`, so a query built for the Wide Capsule tab while
 * carrying the Capsule tab's `600x900` does not fail: it returns portrait art, in the wide tab.
 * That is a real bug this project has already shipped once.
 *
 * The result is deliberately **not** written back to storage. Narrowing on save would discard a
 * selection the moment the user visited a tab that could not display it.
 */
export function pruneToType(type: AssetType, filters: Filters): Filters {
  const keep = (values: string[], allowed: readonly string[]) =>
    values.filter((v) => allowed.includes(v));
  return {
    ...filters,
    styles: keep(filters.styles, STYLES[type]),
    dimensions: keep(filters.dimensions, DIMENSIONS[type].all),
    mimes: keep(filters.mimes, MIMES[type]),
  };
}

/**
 * The query parameters for one tab: clamp to what it offers, widen an empty size list, translate.
 *
 * 🔴 **No size ticked means "any size this tab can show" — it must never mean "send no
 * `dimensions` at all".** Those look equivalent and are not: `grid_p` and `grid_l` are the same
 * SteamGridDB endpoint told apart *only* by `dimensions`, so an unfiltered `grids` query fills
 * the Wide Capsule tab with portrait art. Widening to this tab's own list keeps that separation
 * while leaving the user free to clear every box.
 *
 * Before this, the panel simply refused to let the last size be unticked. That is the same
 * invariant defended in the wrong place: it made a checked box unclickable, which reads as a
 * broken control rather than a protected one.
 *
 * Logos and icons have an empty `all` — every dimension value is a 400 there — so they still
 * send no `dimensions`, which is the only thing those endpoints accept.
 */
export function queryFor(type: AssetType, filters: Filters): QueryParams {
  const pruned = pruneToType(type, filters);
  if (pruned.dimensions.length === 0) {
    return filtersToQuery({ ...pruned, dimensions: [...DIMENSIONS[type].all] });
  }
  return filtersToQuery(pruned);
}
