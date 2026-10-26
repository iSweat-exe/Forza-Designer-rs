import { describe, expect, it } from 'vitest';

import { initialInjectionState, injectionReducer, type InjectionState } from './useInjection';

const running: InjectionState = { ...initialInjectionState, phase: 'running' };

describe('injectionReducer', () => {
  it('tracks scan progress for the template size being searched', () => {
    let s = injectionReducer(running, { type: 'event', event: { kind: 'scanning', layers: 500 } });
    s = injectionReducer(s, { type: 'event', event: { kind: 'scan_progress', scanned: 3, total: 40, hits: 0 } });
    expect(s).toMatchObject({ scanningLayers: 500, scanned: 3, scanTotal: 40 });
  });

  it('a new template size restarts the progress bar', () => {
    let s = injectionReducer(running, { type: 'event', event: { kind: 'scan_progress', scanned: 40, total: 40, hits: 0 } });
    s = injectionReducer(s, { type: 'event', event: { kind: 'scanning', layers: 1500 } });
    expect(s).toMatchObject({ scanned: 0, scanTotal: 0, scanningLayers: 1500 });
  });

  it('RTTI fallback and phases are surfaced', () => {
    let s = injectionReducer(running, { type: 'event', event: { kind: 'rtti_fallback' } });
    s = injectionReducer(s, { type: 'event', event: { kind: 'rtti_phase', phase: 2 } });
    expect(s).toMatchObject({ rttiFallback: true, rttiPhase: 2 });
  });

  it('locating a group switches from scanning to writing', () => {
    let s = injectionReducer(running, { type: 'event', event: { kind: 'scanning', layers: 500 } });
    s = injectionReducer(s, { type: 'event', event: { kind: 'located', slots: 500, method: 'sphere_scan' } });
    s = injectionReducer(s, { type: 'event', event: { kind: 'writing', written: 10, total: 400 } });
    expect(s).toMatchObject({ slots: 500, scanningLayers: null, written: 10, writeTotal: 400 });
  });

  it('start clears the previous run', () => {
    const old: InjectionState = { ...initialInjectionState, phase: 'error', error: 'x', slots: 3 };
    expect(injectionReducer(old, { type: 'start' })).toEqual({ ...initialInjectionState, phase: 'running' });
  });

  it('finished records the report', () => {
    const report = {
      slots: 500, total: 400, written: 399, skipped: 1, bytes: 1, method: 'rtti' as const,
      type_counts: { rotated_ellipse: 399 }, game_build: null, validated_build: '364.933', overrides_applied: [],
    };
    expect(injectionReducer(running, { type: 'finished', report })).toMatchObject({ phase: 'done', written: 399, writeTotal: 400 });
  });
});
