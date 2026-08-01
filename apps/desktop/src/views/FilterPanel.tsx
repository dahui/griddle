/**
 * SteamGridDB's full filter surface.
 *
 * **One filter set, shared by every tab.** What changes per tab is only which *options* exist:
 * sizes and styles have per-endpoint vocabularies, so the panel shows the current tab's and
 * edits the shared set. A size the other tab does not offer stays selected in the background and
 * comes back when you return to that tab — clamping happens when the query is built, not here.
 *
 * The values offered come from the shared tables in `@griddle/shared`, which the Rust side
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
} from '@griddle/shared';
import { useFocusItem } from '../focus';

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

  // Column assignment, in the order `.filter-body`'s flex-wrap lays the groups out.
  //
  // Computed rather than hardcoded, because Style and Size **disappear entirely** on some
  // tabs — logos and icons take no dimension filter at all. Fixed column numbers would leave a
  // gap the pad has to cross with nothing in it, and `closestCol` would then land somewhere
  // arbitrary when entering the panel from above.
  let column = 0;
  const styleCol = styles.length > 0 ? column++ : -1;
  const sizeCol = dimensions.length > 0 ? column++ : -1;
  const formatCol = column++;
  const contentCol = column++;
  const actionsCol = column++;

  return (
    <details
      className="filters"
      open={open}
      onToggle={(e) => setOpen((e.currentTarget as HTMLDetailsElement).open)}
    >
      {/* No "modified" badge: the "Reset filters" button below already appears only when
          something has been changed, so the badge said the same thing twice. */}
      <FilterSummary />

      <div className="filter-body">
        {styles.length > 0 && (
          <Group label="Style">
            {styles.map((s, i) => (
              <Check
                key={s}
                col={styleCol}
                row={i}
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
            {dimensions.map((d, i) => (
              <Check
                key={d}
                col={sizeCol}
                row={i}
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
          {mimes.map((m, i) => (
            <Check
              key={m}
              col={formatCol}
              row={i}
              label={MIME_LABEL[m] ?? m}
              checked={filters.mimes.includes(m)}
              onChange={() => onChange({ ...filters, mimes: toggleIn(filters.mimes, m) })}
            />
          ))}
          <Check
            col={formatCol}
            row={mimes.length}
            label="Animated"
            checked={filters.animated}
            onChange={() => onChange({ ...filters, animated: !filters.animated })}
          />
          <Check
            col={formatCol}
            row={mimes.length + 1}
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
            col={contentCol}
            row={0}
            label="Untagged"
            checked={filters.untagged}
            onChange={() => onChange({ ...filters, untagged: !filters.untagged })}
          />
          <Check
            col={contentCol}
            row={1}
            label="Adult"
            checked={filters.adult}
            onChange={() => onChange({ ...filters, adult: !filters.adult })}
          />
          <Check
            col={contentCol}
            row={2}
            label="Humor"
            checked={filters.humor}
            onChange={() => onChange({ ...filters, humor: !filters.humor })}
          />
          <Check
            col={contentCol}
            row={3}
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
          <ActionButton
            row={0}
            col={actionsCol}
            onClick={onPickGame}
            title="Change which SteamGridDB game this artwork comes from"
          >
            {gameLabel ? `Game: ${gameLabel}` : 'Wrong game?'}
          </ActionButton>
          {/* Appears only once something has been changed, and vanishes the instant it is used —
              taking the focus with it. `nearest` is what puts focus back on "Wrong game?" above
              rather than at the top of the page. */}
          {modified && (
            <ActionButton row={1} col={actionsCol} onClick={onReset}>
              Reset filters
            </ActionButton>
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
  col,
  row,
}: {
  label: string;
  checked: boolean;
  onChange: () => void;
  col: number;
  row: number;
}) {
  const { ref, focused } = useFocusItem<HTMLInputElement>('filters', row, col);
  return (
    <label className={`toggle${focused ? ' focused' : ''}`}>
      <input ref={ref} type="checkbox" checked={checked} onChange={onChange} />
      {label}
    </label>
  );
}

/** The panel's disclosure triangle — its own section, so the panel can be opened from the pad. */
function FilterSummary() {
  const { ref, focused } = useFocusItem<HTMLElement>('filters-summary', 0, 0);
  return (
    <summary ref={ref} className={focused ? 'focused' : undefined}>
      Filters
    </summary>
  );
}

function ActionButton({
  row,
  col,
  onClick,
  title,
  children,
}: {
  row: number;
  col: number;
  onClick: () => void;
  title?: string;
  children: React.ReactNode;
}) {
  const { ref, focused } = useFocusItem<HTMLButtonElement>('filters', row, col);
  return (
    <button
      ref={ref}
      type="button"
      className={`ghost${focused ? ' focused' : ''}`}
      onClick={onClick}
      title={title}
    >
      {children}
    </button>
  );
}
