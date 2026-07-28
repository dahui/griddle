import { defineConfig } from 'vite';

/**
 * The Big Picture bundle is handed to CDP `Runtime.evaluate` as a single string, so it must be
 * ONE self-contained IIFE: no code splitting, no dynamic import, no module preload, no
 * external references. Any of those would produce a bundle that loads fine in a browser and
 * fails silently inside Steam.
 */
export default defineConfig({
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    // Steam's CEF is modern Chromium, but stay conservative — a syntax error here surfaces as
    // "injection did nothing" with no stack trace.
    target: 'chrome100',
    lib: {
      entry: 'src/index.ts',
      formats: ['iife'],
      name: '__SGDB_BUNDLE__',
      fileName: () => 'bpm.js',
    },
    rollupOptions: {
      output: {
        inlineDynamicImports: true,
      },
    },
    minify: false,
    sourcemap: false,
  },
});
