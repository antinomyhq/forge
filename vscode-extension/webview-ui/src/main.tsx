import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { EffectRuntimeProvider } from './presentation/hooks/useRuntimeSimple';
import App from './App';
import './index.css';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <EffectRuntimeProvider>
      <App />
    </EffectRuntimeProvider>
  </StrictMode>
);
