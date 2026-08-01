import React from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { FocusProvider } from './focus';
import './styles.css';

const root = document.getElementById('root');
if (!root) {
  throw new Error('#root missing from index.html');
}

// 🔴 `FocusProvider` sits **above** `App`, not inside its `Shell`. It used to be in `Shell`, which
// put `App` itself outside the context — so the one component that owns which screen is showing
// could not claim the buttons that switch screens.
createRoot(root).render(
  <React.StrictMode>
    <FocusProvider>
      <App />
    </FocusProvider>
  </React.StrictMode>,
);
