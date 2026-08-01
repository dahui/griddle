/**
 * The right-click menu and its items.
 *
 * Kept together because the two halves are a single mechanism: the menu's dismiss listener runs
 * in the capture phase, and whether an item's own `onClick` ever fires depends on that listener
 * leaving clicks inside the menu alone. Separating them hides the coupling.
 */
import { useEffect, useRef, type ReactNode } from 'react';
import { FocusScope, useFocusItem } from '../focus';

/**
 * A right-click menu anchored at the cursor.
 *
 * Closes on Escape, on a click outside it, and on scroll — a menu that outlives what it points
 * at is worse than no menu, because the next click lands on an action the user has stopped
 * looking at. `position: fixed` so the coordinates are viewport-relative.
 *
 * **A click *inside* the menu must not close it here.** The dismiss listener is on `window`
 * in the **capture** phase, so it runs before the click reaches the menu item's own handler. If
 * it closes unconditionally, React unmounts the item mid-dispatch and its `onClick` never fires
 * — every menu action silently does nothing, which is precisely how this shipped the first
 * time. Letting the item's own handler close the menu is what actually runs the action.
 */
export function ContextMenu({
  x,
  y,
  onClose,
  children,
}: {
  x: number;
  y: number;
  onClose: () => void;
  children: ReactNode;
}) {
  const menu = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const closeOutside = (e: Event) => {
      // A click on the menu is the menu being *used*, not dismissed.
      if (e.target instanceof Node && menu.current?.contains(e.target)) return;
      onClose();
    };
    // Scrolling moves what the menu points at, so it always dismisses — no inside/outside test.
    //
    // `capture: true` is why this cannot simply be left alone once a controller can scroll:
    // moving focus calls `scrollIntoView`, which fires this and closes the menu the user is
    // navigating. Keyboard focus movement inside the menu does not scroll the page, so this is
    // correct today; opening the menu *from* the pad is what will need the anchor rework.
    const onScroll = () => onClose();
    // `capture` so the menu still closes when something below stops propagation.
    window.addEventListener('click', closeOutside, true);
    window.addEventListener('contextmenu', closeOutside, true);
    window.addEventListener('scroll', onScroll, true);
    return () => {
      window.removeEventListener('click', closeOutside, true);
      window.removeEventListener('contextmenu', closeOutside, true);
      window.removeEventListener('scroll', onScroll, true);
    };
  }, [onClose]);

  // Escape now comes from the focus scope rather than a window listener of its own. Two such
  // listeners existed — here and in `ArtPreview` — both unconditional, so with a menu open over a
  // preview a single press closed both at once.
  return (
    <FocusScope name="menu" onBack={onClose}>
    <div
      ref={menu}
      className="context-menu"
      role="menu"
      style={{
        // Keep the menu on screen when the click lands near an edge.
        left: Math.min(x, window.innerWidth - 260),
        top: Math.min(y, window.innerHeight - 120),
      }}
    >
      {children}
    </div>
    </FocusScope>
  );
}

/**
 * One item in a context menu, reachable by keyboard.
 *
 * The activation must originate from a node **inside** the menu, or the capture-phase dismiss
 * listener above unmounts the item before its own `onClick` runs. Real DOM focus is what keeps
 * that true for the keyboard: pressing Enter dispatches a click from the item itself, exactly as
 * a mouse would.
 */
export function MenuItem({
  row,
  onSelect,
  children,
}: {
  row: number;
  onSelect: () => void;
  children: ReactNode;
}) {
  const { ref, focused } = useFocusItem<HTMLButtonElement>('menu', row, 0);
  return (
    <button
      ref={ref}
      type="button"
      role="menuitem"
      className={`menu-item${focused ? ' focused' : ''}`}
      onClick={onSelect}
    >
      {children}
    </button>
  );
}
