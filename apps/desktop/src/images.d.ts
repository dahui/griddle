/**
 * Image imports, which Vite turns into a URL string at build time.
 *
 * Declared here rather than by adding `vite/client` to the root `tsconfig.json`, because that
 * file also compiles `packages/shared` -- which is deliberately DOM-free so the focus maths can
 * be unit-tested without a browser. Handing it Vite's ambient types would quietly make
 * `import.meta.env` and asset imports look legitimate in a package that must not have either.
 */
declare module '*.png' {
  const src: string;
  export default src;
}
