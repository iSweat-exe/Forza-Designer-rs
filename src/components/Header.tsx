import { useI18n } from '../i18n/I18nProvider';
import type { Lang } from '../i18n/strings';

export type TabId = 'generate' | 'inject';

interface Props {
  tab: TabId;
  onTab: (tab: TabId) => void;
}

export function Header({ tab, onTab }: Props) {
  const { t, lang, setLang } = useI18n();
  const tabs: { id: TabId; label: string }[] = [
    { id: 'generate', label: t('tabGenerate') },
    { id: 'inject', label: t('tabInject') },
  ];
  return (
    <header className="header">
      <div className="brand">
        <span className="brand-mark" aria-hidden="true" />
        <div>
          <h1>{t('appName')}</h1>
          <p className="muted small">{t('credit')}</p>
        </div>
      </div>
      <nav className="tabs" role="tablist" aria-label="Sections">
        {tabs.map(({ id, label }) => (
          <button
            key={id}
            role="tab"
            id={`tab-${id}`}
            aria-selected={tab === id}
            aria-controls={`panel-${id}`}
            className={tab === id ? 'tab active' : 'tab'}
            onClick={() => onTab(id)}
          >
            {label}
          </button>
        ))}
      </nav>
      <label className="lang">
        <span className="visually-hidden">{t('language')}</span>
        <select value={lang} onChange={(e) => setLang(e.target.value as Lang)}>
          <option value="en">English</option>
          <option value="fr">Français</option>
        </select>
      </label>
    </header>
  );
}
