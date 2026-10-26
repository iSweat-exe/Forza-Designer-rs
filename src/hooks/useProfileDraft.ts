import { useCallback, useState } from 'react';

import type { Profile } from '../api/types';

const DEFAULT_PRESET = 'balanced';

/**
 * Preset selection plus the user's edits on top of it. Edits belong to the
 * preset they were made on: switching presets discards them. Everything is
 * derived during render, so no effect is needed to "seed" the draft.
 */
export function useProfileDraft(presets: Profile[] | null) {
  const [presetName, setPresetName] = useState(DEFAULT_PRESET);
  const [edit, setEdit] = useState<{ preset: string; profile: Profile } | null>(null);

  const base = presets?.find((p) => p.name === presetName) ?? presets?.[0] ?? null;
  const draft = base && edit?.preset === base.name ? edit.profile : base;

  const select = useCallback((name: string) => {
    setPresetName(name);
    setEdit(null);
  }, []);

  const change = useCallback(
    (profile: Profile) => {
      if (base) setEdit({ preset: base.name, profile });
    },
    [base],
  );

  return { presetName: base?.name ?? presetName, draft, select, change };
}
