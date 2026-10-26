import { useCallback, useState } from 'react';

import { api } from '../api/commands';
import { useKeyedAsync } from './useKeyedAsync';

/** The picked source image and its thumbnail/size, loaded from the backend. */
export function useImageSelection() {
  const [path, setPath] = useState<string | null>(null);
  const { data: info, error } = useKeyedAsync(path, api.describeImage);
  const select = useCallback((p: string) => setPath(p), []);
  return { path, info, error, select };
}
