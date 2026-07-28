import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Tauri drives this dev server on a fixed port; `strictPort` makes a conflict a loud failure
// rather than a silently-wrong window pointing at someone else's dev server.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    // WebView2 is evergreen Chromium, so there is no reason to down-level.
    target: 'chrome110',
    sourcemap: true,
  },
});
