import { useEffect, useState } from 'react';

import { errorMessage } from '../api/commands';

interface Keyed<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
}

interface Settled<K, T> {
  key: K;
  data: T | null;
  error: string | null;
}

/**
 * Loads a value for `key` (e.g. a file path). Results that belong to a
 * superseded key are never exposed, so a slow response can't overwrite a newer
 * one. `load` must be a stable reference. `key === null` means "nothing to load".
 */
export function useKeyedAsync<K extends string, T>(key: K | null, load: (key: K) => Promise<T>): Keyed<T> {
  const [settled, setSettled] = useState<Settled<K, T> | null>(null);

  useEffect(() => {
    if (key === null) return;
    let cancelled = false;
    load(key)
      .then((data) => !cancelled && setSettled({ key, data, error: null }))
      .catch((e: unknown) => !cancelled && setSettled({ key, data: null, error: errorMessage(e) }));
    return () => {
      cancelled = true;
    };
  }, [key, load]);

  const current = settled !== null && settled.key === key ? settled : null;
  return { data: current?.data ?? null, error: current?.error ?? null, loading: key !== null && current === null };
}
