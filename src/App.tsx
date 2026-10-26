import { useState } from 'react';

import type { DocumentInfo } from './api/types';
import { GenerateTab } from './components/GenerateTab';
import { Header, type TabId } from './components/Header';
import { InjectTab } from './components/InjectTab';

export default function App() {
  const [tab, setTab] = useState<TabId>('generate');
  // The document being worked on is shared: Generate produces it, Inject consumes it.
  const [document, setDocument] = useState<DocumentInfo | null>(null);

  return (
    <div className="app">
      <Header tab={tab} onTab={setTab} />
      <main className="main">
        {/* Both tabs stay mounted so a running generation keeps its UI state. */}
        <div hidden={tab !== 'generate'}>
          <GenerateTab onDocument={setDocument} onGoInject={() => setTab('inject')} />
        </div>
        <div hidden={tab !== 'inject'}>
          <InjectTab document={document} onDocument={setDocument} />
        </div>
      </main>
    </div>
  );
}
