/**
 * SteamGridDB's full filter surface.
 *
 * **One filter set, shared by every tab.** What changes per tab is only which *options* exist:
 * sizes and styles have per-endpoint vocabularies, so the panel shows the current tab's and
 * edits the shared set. A size the other tab does not offer stays selected in the background and
 * comes back when you return to that tab — clamping happens when the query is built, not here.
 *
 * The values offered come from the shared tables in `@sgdb/shared`, which the Rust side
 * validates against. They are held in step by `packages/shared/fixtures/filter-vocabulary.json`,
 * asserted by tests in both languages, because a value offered here but unknown to Rust is
 * refused locally and reads as a filter that silently returns nothing.
 */
import { useState } from 'react';
import {
  DIMENSIONS,
  MIMES,
  STYLES,
  STYLE_LABEL,
  isDefault,
  type AssetType,
  type Filters,
} from '@sgdb/shared';

const MIME_LABEL: Record<string, string> = {
  'image/png': 'PNG',
  'image/jpeg': 'JPEG',
  'image/webp': 'WebP',
  'image/vnd.microsoft.icon': 'ICO',
};

export function FilterPanel({
  assetType,
  filters,
  onChange,
  onReset,
  onPickGame,
  gameLabel,
}: {
  assetType: AssetType;
  filters: Filters;
  onChange: (next: Filters) => void;
  onReset: () => void;
  onPickGame: () => void;
  gameLabel: string | null;
}) {
  const styles = STYLES[assetType];
  const dimensions = DIMENSIONS[assetType].all;
  const mimes = MIMES[assetType];
  const modified = !isDefault(filters);
  // Counted among the *visible* options only: the shared set holds values for every tab, so a
  // plain `filters.styles.length` is never zero even when this tab has nothing ticked.
  const ticked = (options: readonly string[], chosen: string[]) =>
    options.filter((o) => chosen.includes(o)).length;
  const checkedStyles = ticked(styles, filters.styles);
  const checkedSizes = ticked(dimensions, filters.dimensions);
  const checkedMimes = ticked(mimes, filters.mimes);

  // React *controls* `open` on a <details>, so binding it straight to `modified` would slam the
  // panel shut the instant the user clicked "Reset filters" — while their cursor was still
  // inside it. Seeded from `modified` once, then owned by the user.
  const [open, setOpen] = useState(modified);

  const toggleIn = (list: string[], value: string) =>
    list.includes(value) ? list.filter((v) => v !== value) : [...list, value];

  return (
    <details
      className="filters"
      open={open}
      onToggle={(e) => setOpen((e.currentTarget as HTMLDetailsElement).open)}
    >
      {/* No "modified" badge: the "Reset filters" button below already appears only when
          something has been changed, so the badge said the same thing twice. */}
      <summary>Filters</summary>

      <div className="filter-body">
        {styles.length > 0 && (
          <Group label="Style">
            {styles.map((s) => (
              <Check
                key={s}
                label={STYLE_LABEL[s] ?? s}
                checked={filters.styles.includes(s)}
                onChange={() => onChange({ ...filters, styles: toggleIn(filters.styles, s) })}
              />
            ))}
            {checkedStyles === 0 && <p className="hint filter-hint">Any style.</p>}
          </Group>
        )}

        {/* Logos and icons take no dimension filter at all — every value is a 400 — so the
            group is hidden rather than rendered empty. */}
        {dimensions.length > 0 && (
          <Group label="Size">
            {dimensions.map((d) => (
              <Check
                key={d}
                label={d}
                checked={filters.dimensions.includes(d)}
                onChange={() =>
                  onChange({ ...filters, dimensions: toggleIn(filters.dimensions, d) })
                }
              />
            ))}
            {/* Every box is now unticked, which `queryFor` widens to this tab's full list. The
                note is what makes that visible — an unticked group that quietly shows everything
                is otherwise indistinguishable from one that is broken. */}
            {checkedSizes === 0 && <p className="hint filter-hint">Any size.</p>}
          </Group>
        )}

        <Group label="Format">
          {mimes.map((m) => (
            <Check
              key={m}
              label={MIME_LABEL[m] ?? m}
              checked={filters.mimes.includes(m)}
              onChange={() => onChange({ ...filters, mimes: toggleIn(filters.mimes, m) })}
            />
          ))}
          <Check
            label="Animated"
            checked={filters.animated}
            onChange={() => onChange({ ...filters, animated: !filters.animated })}
          />
          <Check
            label="Static"
            checked={filters.static}
            onChange={() => onChange({ ...filters, static: !filters.static })}
          />
          {/* These two used to refuse to let the last one be unticked, on the grounds that
              `filtersToQuery` omits `types` when both are off — so "neither" quietly means "any",
              the opposite of what it looks like. That reasoning was right and the remedy was
              wrong: it made a ticked box unclickable, which reads as broken rather than
              protected. Saying what the state actually does fixes the misreading without taking
              the control away, and matches how Size and Format behave right beside it. */}
          {checkedMimes === 0 && <p className="hint filter-hint">Any file format.</p>}
          {!filters.animated && !filters.static && (
            <p className="hint filter-hint">Any type — animated and static both shown.</p>
          )}
        </Group>

        <Group label="Content">
          <Check
            label="Untagged"
            checked={filters.untagged}
            onChange={() => onChange({ ...filters, untagged: !filters.untagged })}
          />
          <Check
            label="Adult"
            checked={filters.adult}
            onChange={() => onChange({ ...filters, adult: !filters.adult })}
          />
          <Check
            label="Humor"
            checked={filters.humor}
            onChange={() => onChange({ ...filters, humor: !filters.humor })}
          />
          <Check
            label="Epilepsy"
            checked={filters.epilepsy}
            onChange={() => onChange({ ...filters, epilepsy: !filters.epilepsy })}
          />
          {/* The inversion is genuinely counter-intuitive, and a user who turns Untagged off
              and sees their results shrink deserves to know why. */}
          <p className="hint filter-hint">
            {filters.untagged
              ? 'Untagged artwork, plus anything ticked above.'
              : 'Only artwork carrying one of the ticked tags — untagged art is hidden.'}
          </p>
        </Group>

        <div className="filter-actions">
          <button
            type="button"
            className="ghost"
            onClick={onPickGame}
            title="Change which SteamGridDB game this artwork comes from"
          >
            {gameLabel ? `Game: ${gameLabel}` : 'Wrong game?'}
          </button>
          {modified && (
            <button type="button" className="ghost" onClick={onReset}>
              Reset filters
            </button>
          )}
        </div>
      </div>
    </details>
  );
}

function Group({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <fieldset className="filter-group">
      <legend>{label}</legend>
      {children}
    </fieldset>
  );
}

/**
 * No `disabled` prop, on purpose.
 *
 * Every box in this panel is always clickable. Greying one out to protect an invariant was the
 * cause of the only filter bug ever reported here — the invariants are defended in `queryFor`
 * and by the notes above, where they cost the user nothing.
 */
function Check({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: () => void;
}) {
  return (
    <label className="toggle">
      <input type="checkbox" checked={checked} onChange={onChange} />
      {label}
    </label>
  );
}
