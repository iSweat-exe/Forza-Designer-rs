import { useCallback, useEffect, useState } from 'react';

import { errorMessage } from '../api/commands';

interface Resource<T> {
  data: T | null;
  loading: boolean;
  error: string | null;
  reload: () => void;
}

interface Settled<T> {
  tick: number;
  data: T | null;
  error: string | null;
}

/**
 * Runs `load` on mount and whenever `reload()` is called. `load` must be a
 * stable reference (a module-level function or a memoised callback). Data from
 * the previous load stays visible while a reload is in flight.
 */
export function useAsyncResource<T>(load: () => Promise<T>): Resource<T> {
  const [tick, setTick] = useState(0);
  const [settled, setSettled] = useState<Settled<T> | null>(null);

  useEffect(() => {
    let cancelled = false;
    load()
      .then((data) => !cancelled && setSettled({ tick, data, error: null }))
      .catch((e: unknown) => !cancelled && setSettled((prev) => ({ tick, data: prev?.data ?? null, error: errorMessage(e) })));
    return () => {
      cancelled = true;
    };
  }, [load, tick]);

  const reload = useCallback(() => setTick((t) => t + 1), []);
  return {
    data: settled?.data ?? null,
    error: settled?.error ?? null,
    loading: settled === null || settled.tick !== tick,
    reload,
  };
}
