import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { deferred } from '../test-utils';
import { useKeyedAsync } from './useKeyedAsync';

describe('useKeyedAsync', () => {
  it('does nothing for a null key', () => {
    const load = vi.fn();
    const { result } = renderHook(() => useKeyedAsync(null, load));
    expect(load).not.toHaveBeenCalled();
    expect(result.current).toEqual({ data: null, error: null, loading: false });
  });

  it('loads for a key and exposes the data', async () => {
    const load = vi.fn().mockResolvedValue('thumb');
    const { result } = renderHook(() => useKeyedAsync('a.png', load));
    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.data).toBe('thumb'));
    expect(result.current.loading).toBe(false);
  });

  it('never shows a slow result for a superseded key', async () => {
    const slow = deferred<string>();
    const fast = deferred<string>();
    const load = vi.fn((key: string) => (key === 'old' ? slow.promise : fast.promise));
    const { result, rerender } = renderHook(({ k }) => useKeyedAsync(k, load), { initialProps: { k: 'old' } });
    rerender({ k: 'new' });
    await act(async () => fast.resolve('new-data'));
    await act(async () => slow.resolve('old-data'));
    expect(result.current.data).toBe('new-data');
  });

  it('hides the previous key\'s data while the next one loads', async () => {
    const next = deferred<string>();
    const load = vi.fn((key: string) => (key === 'a' ? Promise.resolve('A') : next.promise));
    const { result, rerender } = renderHook(({ k }) => useKeyedAsync(k, load), { initialProps: { k: 'a' } });
    await waitFor(() => expect(result.current.data).toBe('A'));
    rerender({ k: 'b' });
    expect(result.current).toMatchObject({ data: null, loading: true });
    await act(async () => next.resolve('B'));
    expect(result.current.data).toBe('B');
  });

  it('surfaces failures as a message', async () => {
    const { result } = renderHook(() => useKeyedAsync('x', () => Promise.reject('bad file')));
    await waitFor(() => expect(result.current.error).toBe('bad file'));
    expect(result.current.loading).toBe(false);
  });
});
