/**
 * The right-hand end of the app's nav row, offered to whichever view is showing.
 *
 * `.tabs` has always been `justify-content: space-between` with a single child, so the row was
 * built for a second group opposite the Library/Settings tabs and never had one. The tile-size
 * control belongs there: it is per-view state, so the view has to own it, but it reads as part of
 * the app frame rather than as one more thing crowding the toolbar below.
 *
 * A portal rather than lifted state, because the *target* of the zoom depends on which asset tab
 * is open — knowledge that lives inside `AssetBrowser` and would have to be threaded back up
 * through `App` for no other reason. The portal keeps ownership where the state is.
 *
 * Document order still works out: React portals render into the anchor's real DOM position, so
 * the focus model sees these controls in the nav row where they appear, not where they are
 * written.
 */
import { createContext } from 'react';

export const NavSlotCtx = createContext<HTMLElement | null>(null);
