import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { makeProfile } from '../test-utils';
import { useProfileDraft } from './useProfileDraft';

const presets = [makeProfile({ name: 'fast', stop_at: 1500 }), makeProfile({ name: 'balanced', stop_at: 3000 })];

describe('useProfileDraft', () => {
  it('has no draft until presets are loaded', () => {
    const { result } = renderHook(() => useProfileDraft(null));
    expect(result.current.draft).toBeNull();
  });

  it('starts on the balanced preset', () => {
    const { result } = renderHook(() => useProfileDraft(presets));
    expect(result.current.presetName).toBe('balanced');
    expect(result.current.draft?.stop_at).toBe(3000);
  });

  it('falls back to the first preset when the default is missing', () => {
    const { result } = renderHook(() => useProfileDraft([presets[0]!]));
    expect(result.current.presetName).toBe('fast');
  });

  it('keeps edits for the current preset', () => {
    const { result } = renderHook(() => useProfileDraft(presets));
    act(() => result.current.change({ ...result.current.draft!, stop_at: 42 }));
    expect(result.current.draft?.stop_at).toBe(42);
  });

  it('discards edits when another preset is selected', () => {
    const { result } = renderHook(() => useProfileDraft(presets));
    act(() => result.current.change({ ...result.current.draft!, stop_at: 42 }));
    act(() => result.current.select('fast'));
    expect(result.current.draft?.stop_at).toBe(1500);
    act(() => result.current.select('balanced'));
    expect(result.current.draft?.stop_at).toBe(3000);
  });

  it('adopts presets that arrive after mount', () => {
    const { result, rerender } = renderHook(({ p }) => useProfileDraft(p), { initialProps: { p: null as typeof presets | null } });
    rerender({ p: presets });
    expect(result.current.draft?.name).toBe('balanced');
  });
});
