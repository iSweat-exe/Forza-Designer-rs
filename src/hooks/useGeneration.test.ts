import { describe, expect, it } from 'vitest';

import { generationReducer, initialGenerationState, type GenerationState } from './useGeneration';

const running: GenerationState = { ...initialGenerationState, phase: 'running', total: 100, startedAt: 1 };

describe('generationReducer', () => {
  it('start resets previous results and records the total', () => {
    const dirty: GenerationState = { ...initialGenerationState, phase: 'done', shapes: 50, warnings: ['x'] };
    const next = generationReducer(dirty, { type: 'start', total: 300, now: 42 });
    expect(next).toMatchObject({ phase: 'running', total: 300, shapes: 0, warnings: [], startedAt: 42 });
  });

  it('progress and preview events update the live view', () => {
    let s = generationReducer(running, { type: 'event', event: { kind: 'progress', shapes: 7, total: 100, rms: 12.5 } });
    s = generationReducer(s, { type: 'event', event: { kind: 'preview', png: 'data:image/png;base64,AAA' } });
    expect(s).toMatchObject({ shapes: 7, rms: 12.5, preview: 'data:image/png;base64,AAA' });
  });

  it('counts checkpoints and collects warnings', () => {
    let s = generationReducer(running, { type: 'event', event: { kind: 'checkpoint', path: 'a.json' } });
    s = generationReducer(s, { type: 'event', event: { kind: 'warning', message: 'disk full' } });
    expect(s.checkpoints).toBe(1);
    expect(s.warnings).toEqual(['disk full']);
  });

  it('pause only applies while a run is active', () => {
    expect(generationReducer(running, { type: 'paused', paused: true }).phase).toBe('paused');
    expect(generationReducer(initialGenerationState, { type: 'paused', paused: true }).phase).toBe('idle');
    const paused = generationReducer(running, { type: 'paused', paused: true });
    expect(generationReducer(paused, { type: 'paused', paused: false }).phase).toBe('running');
  });

  it('finished adopts the summary numbers', () => {
    const s = generationReducer(running, {
      type: 'finished',
      summary: { path: 'o.json', shapes: 80, rms: 9.1, stopped_early: true, message: null },
    });
    expect(s).toMatchObject({ phase: 'done', shapes: 80, rms: 9.1 });
  });

  it('failed keeps the message', () => {
    expect(generationReducer(running, { type: 'failed', error: 'boom' })).toMatchObject({ phase: 'error', error: 'boom' });
  });
});
