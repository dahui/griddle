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
  // Counted among the *visible* options only. The shared set holds sizes for every tab, so
  // `filters.dimensions.length` would never reach one and the last-one guard below would never
  // engage.
  const checkedSizes = dimensions.filter((d) => filters.dimensions.includes(d)).length;

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
          </Group>
        )}

        {/* Logos and icons take no dimension filter at all — every value is a 400 — so the
            group is hidden rather than rendered empty. */}
        {dimensions.length > 0 && (
          <Group label="Size">
            {dimensions.map((d) => {
              const checked = filters.dimensions.includes(d);
              return (
                <Check
                  key={d}
                  label={d}
                  checked={checked}
                  // Unticking the last size is prevented rather than tolerated. An empty
                  // dimension set makes the backend restore this tab's defaults — it has to,
                  // since `grid_p` and `grid_l` are the same endpoint and an unfiltered `grids`
                  // query fills the Wide tab with portrait art. So the control would appear to
                  // do nothing at all, which is worse than being unavailable.
                  disabled={checked && checkedSizes === 1}
                  onChange={() =>
                    onChange({ ...filters, dimensions: toggleIn(filters.dimensions, d) })
                  }
                />
              );
            })}
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
          {/* Unchecking the last of these is prevented rather than tolerated: `filtersToQuery`
              omits `types` entirely when both are off, which quietly means "no filter" — the
              opposite of what unticking both looks like it should do. */}
          <Check
            label="Animated"
            checked={filters.animated}
            disabled={filters.animated && !filters.static}
            onChange={() => onChange({ ...filters, animated: !filters.animated })}
          />
          <Check
            label="Static"
            checked={filters.static}
            disabled={filters.static && !filters.animated}
            onChange={() => onChange({ ...filters, static: !filters.static })}
          />
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
              ? 'Showing untagged artwork, plus whichever tags are ticked above.'
              : 'Showing only artwork that carries one of the ticked tags. Untagged artwork is hidden.'}
          </p>
        </Group>

        <div className="filter-actions">
          <button type="button" className="ghost" onClick={onPickGame}>
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

function Check({
  label,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: () => void;
}) {
  return (
    <label className="toggle">
      <input type="checkbox" checked={checked} disabled={disabled} onChange={onChange} />
      {label}
    </label>
  );
}
