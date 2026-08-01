/**
 * The DOM half of spatial focus navigation. The arithmetic lives in `@griddle/shared/focusgrid`.
 *
 * Three jobs this does that the pure module cannot:
 *
 * 1. **Measure wrapping grids.** `repeat(auto-fill, minmax(9.5rem, 1fr))` resolves against the
 *    window, and the asset grid changes its `minmax` per tab as well, so the column count is only
 *    knowable from the laid-out DOM. Children are grouped by `offsetTop`.
 * 2. **Order sections by document position**, rather than making every call site pass an index
 *    that would then have to be kept in step by hand.
 * 3. **Move real DOM focus**, not just a highlight. `el.focus()` means Enter, Space and typing
 *    keep working natively and assistive technology follows along — this layer only decides
 *    *which* element is focused, never what activating it does.
 *
 * **Enter and Space are deliberately not intercepted.** Because focus is real, the browser
 * already fires `click` on a focused `<button>` for both. Handling them here too would fire every
 * action twice, and that bug looks like "the app applied the artwork, then applied it again".
 *
 * Three files: `model.ts` is everything that is not React, `provider.tsx` holds the state and the
 * input listeners, `hooks.tsx` is what views call.
 */
export { SCREEN_DEPTH, type NavAction, type ScreenActions } from './model';
export { FocusProvider } from './provider';
export {
  FocusScope,
  useFocusGrid,
  useFocusGridItem,
  useFocusItem,
  useScreenActions,
} from './hooks';
