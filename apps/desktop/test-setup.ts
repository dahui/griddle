/**
 * Gives `bun test` a DOM, for the handful of tests that need real elements and real focus.
 *
 * Preloaded via `bunfig.toml` rather than imported per file, because the registration has to
 * happen before React is imported — React reads `document` at module scope, and a file that
 * imports it first gets a React bound to a window that does not exist yet.
 *
 * Most of this project's tests are pure and need none of this: the focus *arithmetic* lives in
 * `@griddle/shared/focusgrid` precisely so it can be tested without a browser. What needs a DOM
 * is the wiring — that a control registers once and not on every cursor move, which is a claim
 * about React's rendering and cannot be made anywhere else.
 */
import { GlobalRegistrator } from '@happy-dom/global-registrator';

GlobalRegistrator.register();

// happy-dom has no layout engine, so every element reports `offsetTop === 0` and the real
// `ResizeObserver` is absent. Tests that need a column count set it explicitly instead.
if (!('ResizeObserver' in globalThis)) {
  // @ts-expect-error -- assigning a test double onto the global.
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
}
