import { useId } from 'react';

import { SHAPE_KINDS, type Profile, type ShapeKind } from '../api/types';
import { useI18n } from '../i18n/I18nProvider';
import type { StringKey } from '../i18n/strings';
import { clamp } from '../lib';

const PRESET_LABELS: Record<string, StringKey> = {
  fast: 'profileFast',
  balanced: 'profileBalanced',
  best_quality: 'profileBest',
  extreme_quality: 'profileExtreme',
};

interface Props {
  presets: Profile[];
  /** Name of the preset the draft started from. */
  presetName: string;
  draft: Profile;
  disabled: boolean;
  onPreset: (name: string) => void;
  onChange: (next: Profile) => void;
}

function NumberField({
  label,
  value,
  min,
  max,
  disabled,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  disabled: boolean;
  onChange: (n: number) => void;
}) {
  const id = useId();
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      <input
        id={id}
        type="number"
        inputMode="numeric"
        min={min}
        max={max}
        value={value}
        disabled={disabled}
        onChange={(e) => {
          const n = e.target.valueAsNumber;
          if (Number.isFinite(n)) onChange(clamp(Math.round(n), min, max));
        }}
      />
    </div>
  );
}

export function ProfileForm({ presets, presetName, draft, disabled, onPreset, onChange }: Props) {
  const { t } = useI18n();
  const set = <K extends keyof Profile>(key: K, value: Profile[K]) => onChange({ ...draft, [key]: value });

  const toggleKind = (kind: ShapeKind) => {
    const has = draft.shape_types.includes(kind);
    const next = has ? draft.shape_types.filter((k) => k !== kind) : [...draft.shape_types, kind];
    // At least one type must stay selected.
    if (next.length > 0) set('shape_types', next);
  };

  return (
    <div className="stack">
      <fieldset className="fieldset" disabled={disabled}>
        <legend>{t('profile')}</legend>
        <div className="chips" role="radiogroup" aria-label={t('profile')}>
          {presets.map((p) => (
            <button
              key={p.name}
              type="button"
              role="radio"
              aria-checked={presetName === p.name}
              title={p.description}
              className={presetName === p.name ? 'chip active' : 'chip'}
              onClick={() => onPreset(p.name)}
            >
              {PRESET_LABELS[p.name] ? t(PRESET_LABELS[p.name] as StringKey) : p.name}
            </button>
          ))}
        </div>
      </fieldset>

      <NumberField label={t('stopAt')} value={draft.stop_at} min={1} max={10000} disabled={disabled} onChange={(n) => set('stop_at', n)} />

      <fieldset className="fieldset" disabled={disabled}>
        <legend>{t('shapeTypes')}</legend>
        <div className="chips">
          {SHAPE_KINDS.map((kind) => (
            <label key={kind} className={draft.shape_types.includes(kind) ? 'chip active' : 'chip'}>
              <input
                type="checkbox"
                className="visually-hidden"
                checked={draft.shape_types.includes(kind)}
                onChange={() => toggleKind(kind)}
              />
              {t(`shape_${kind}` as StringKey)}
            </label>
          ))}
        </div>
      </fieldset>

      <details className="advanced">
        <summary>{t('advanced')}</summary>
        <div className="grid2">
          <NumberField label={t('randomSamples')} value={draft.random_samples} min={1} max={100000} disabled={disabled} onChange={(n) => set('random_samples', n)} />
          <NumberField label={t('mutatedSamples')} value={draft.mutated_samples} min={1} max={100000} disabled={disabled} onChange={(n) => set('mutated_samples', n)} />
          <NumberField label={t('maxResolution')} value={draft.max_resolution} min={64} max={8192} disabled={disabled} onChange={(n) => set('max_resolution', n)} />
          <NumberField label={t('threads')} value={draft.max_threads} min={0} max={256} disabled={disabled} onChange={(n) => set('max_threads', n)} />
        </div>
      </details>
    </div>
  );
}
