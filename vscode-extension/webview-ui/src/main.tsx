import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { EffectRuntimeProvider } from './presentation/hooks/useRuntime';
import App from './App';
import './index.css';

console.log('[main.tsx] ===== INITIALIZING FORGE WEBVIEW v2.0 (JSON-RPC) =====');

// Add global message listener to debug ALL messages
window.addEventListener('message', (event) => {
  console.log('[main.tsx] RAW MESSAGE RECEIVED:', event.data);
});

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <EffectRuntimeProvider>
      <App />
    </EffectRuntimeProvider>
  </StrictMode>
);
