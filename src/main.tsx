import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import App from './App';
import { ErrorBoundary } from './components/ErrorBoundary';
import { I18nProvider } from './i18n/I18nProvider';
import './styles.css';

const root = document.getElementById('root');
if (!root) throw new Error('missing #root element');

createRoot(root).render(
  <StrictMode>
    <I18nProvider>
      <ErrorBoundary
        fallback={(error, reset) => (
          <div role="alert" className="card stack" style={{ margin: 24 }}>
            <strong>{error.message}</strong>
            <button className="btn" onClick={reset}>
              OK
            </button>
          </div>
        )}
      >
        <App />
      </ErrorBoundary>
    </I18nProvider>
  </StrictMode>,
);
