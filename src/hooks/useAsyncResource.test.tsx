import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useAsyncResource } from './useAsyncResource';

describe('useAsyncResource', () => {
  it('loads once on mount', async () => {
    const load = vi.fn().mockResolvedValue([1, 2]);
    const { result } = renderHook(() => useAsyncResource(load));
    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.data).toEqual([1, 2]));
    expect(result.current.loading).toBe(false);
    expect(load).toHaveBeenCalledTimes(1);
  });

  it('reload fetches again and keeps the old data visible meanwhile', async () => {
    const load = vi.fn().mockResolvedValueOnce('first').mockResolvedValueOnce('second');
    const { result } = renderHook(() => useAsyncResource(load));
    await waitFor(() => expect(result.current.data).toBe('first'));
    act(() => result.current.reload());
    expect(result.current).toMatchObject({ data: 'first', loading: true });
    await waitFor(() => expect(result.current.data).toBe('second'));
  });

  it('reports a failure and keeps earlier data', async () => {
    const load = vi.fn().mockResolvedValueOnce('ok').mockRejectedValueOnce(new Error('offline'));
    const { result } = renderHook(() => useAsyncResource(load));
    await waitFor(() => expect(result.current.data).toBe('ok'));
    act(() => result.current.reload());
    await waitFor(() => expect(result.current.error).toBe('offline'));
    expect(result.current.data).toBe('ok');
  });

  it('ignores a result that arrives after unmount', async () => {
    let resolve!: (v: string) => void;
    const load = vi.fn(() => new Promise<string>((r) => (resolve = r)));
    const { unmount } = renderHook(() => useAsyncResource(load));
    unmount();
    await act(async () => resolve('late')); // must not throw or warn
  });
});
