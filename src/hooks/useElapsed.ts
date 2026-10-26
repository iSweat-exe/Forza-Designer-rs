import { useEffect, useState } from 'react';

/** Seconds since `startedAt`, ticking once a second while `active`. */
export function useElapsedSeconds(startedAt: number | null, active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [active]);
  return startedAt === null ? 0 : Math.max(0, (now - startedAt) / 1000);
}
