import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { GenerateRequest, GenerationEvent } from '../api/types';
import { deferred, makeProfile } from '../test-utils';
import { useGeneration } from './useGeneration';

const api = vi.hoisted(() => ({ generate: vi.fn(), stopGeneration: vi.fn(), setGenerationPaused: vi.fn() }));
vi.mock('../api/commands', () => ({ api, errorMessage: (e: unknown) => String(e) }));

const request: GenerateRequest = { imagePath: 'a.png', profile: makeProfile({ stop_at: 10 }), stickerMode: false };
const summary = { path: 'a.json', shapes: 10, rms: 5, stopped_early: false, message: null };

describe('useGeneration', () => {
  beforeEach(() => {
    api.generate.mockReset();
    api.stopGeneration.mockReset().mockResolvedValue(undefined);
    api.setGenerationPaused.mockReset().mockResolvedValue(undefined);
  });

  it('streams events into state and finishes with the summary', async () => {
    api.generate.mockImplementation(async (_req: GenerateRequest, onEvent: (e: GenerationEvent) => void) => {
      onEvent({ kind: 'progress', shapes: 4, total: 10, rms: 30 });
      return summary;
    });
    const { result } = renderHook(() => useGeneration());
    let returned: unknown;
    await act(async () => {
      returned = await result.current.start(request);
    });
    expect(returned).toEqual(summary);
    expect(result.current.state).toMatchObject({ phase: 'done', shapes: 10, rms: 5 });
  });

  it('turns a rejected run into an error state', async () => {
    api.generate.mockRejectedValue('no such file');
    const { result } = renderHook(() => useGeneration());
    let returned: unknown = 'unset';
    await act(async () => {
      returned = await result.current.start(request);
    });
    expect(returned).toBeNull();
    expect(result.current.state).toMatchObject({ phase: 'error', error: 'no such file' });
  });

  it('ignores a second start while one is running', async () => {
    const first = deferred<typeof summary>();
    api.generate.mockReturnValue(first.promise);
    const { result } = renderHook(() => useGeneration());
    act(() => void result.current.start(request));
    await act(async () => {
      expect(await result.current.start(request)).toBeNull();
    });
    expect(api.generate).toHaveBeenCalledTimes(1);
    await act(async () => first.resolve(summary));
    await waitFor(() => expect(result.current.state.phase).toBe('done'));
  });

  it('pause and stop go to the backend', async () => {
    api.generate.mockReturnValue(new Promise(() => undefined));
    const { result } = renderHook(() => useGeneration());
    act(() => void result.current.start(request));
    await act(async () => result.current.setPaused(true));
    expect(api.setGenerationPaused).toHaveBeenCalledWith(true);
    expect(result.current.state.phase).toBe('paused');
    act(() => result.current.stop());
    expect(api.stopGeneration).toHaveBeenCalled();
  });

  it('can run again after finishing', async () => {
    api.generate.mockResolvedValue(summary);
    const { result } = renderHook(() => useGeneration());
    await act(async () => void (await result.current.start(request)));
    await act(async () => void (await result.current.start(request)));
    expect(api.generate).toHaveBeenCalledTimes(2);
  });
});
