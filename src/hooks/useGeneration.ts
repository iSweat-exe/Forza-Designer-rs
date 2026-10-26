import { useCallback, useReducer, useRef } from 'react';

import { api, errorMessage } from '../api/commands';
import type { GenerateRequest, GenerationEvent, GenerationSummary } from '../api/types';

export type GenerationPhase = 'idle' | 'running' | 'paused' | 'done' | 'error';

export interface GenerationState {
  phase: GenerationPhase;
  shapes: number;
  total: number;
  rms: number | null;
  preview: string | null;
  checkpoints: number;
  warnings: string[];
  summary: GenerationSummary | null;
  error: string | null;
  startedAt: number | null;
}

export const initialGenerationState: GenerationState = {
  phase: 'idle',
  shapes: 0,
  total: 0,
  rms: null,
  preview: null,
  checkpoints: 0,
  warnings: [],
  summary: null,
  error: null,
  startedAt: null,
};

export type GenerationAction =
  | { type: 'start'; total: number; now: number }
  | { type: 'event'; event: GenerationEvent }
  | { type: 'paused'; paused: boolean }
  | { type: 'finished'; summary: GenerationSummary }
  | { type: 'failed'; error: string }
  | { type: 'reset' };

export function generationReducer(state: GenerationState, action: GenerationAction): GenerationState {
  switch (action.type) {
    case 'start':
      return { ...initialGenerationState, phase: 'running', total: action.total, startedAt: action.now };
    case 'event': {
      const e = action.event;
      switch (e.kind) {
        case 'progress':
          return { ...state, shapes: e.shapes, total: e.total, rms: e.rms };
        case 'preview':
          return { ...state, preview: e.png };
        case 'checkpoint':
          return { ...state, checkpoints: state.checkpoints + 1 };
        case 'warning':
          return { ...state, warnings: [...state.warnings, e.message] };
        case 'started':
          return state;
      }
      return state;
    }
    case 'paused':
      return state.phase === 'running' || state.phase === 'paused'
        ? { ...state, phase: action.paused ? 'paused' : 'running' }
        : state;
    case 'finished':
      return { ...state, phase: 'done', summary: action.summary, shapes: action.summary.shapes, rms: action.summary.rms };
    case 'failed':
      return { ...state, phase: 'error', error: action.error };
    case 'reset':
      return initialGenerationState;
  }
}

export function useGeneration() {
  const [state, dispatch] = useReducer(generationReducer, initialGenerationState);
  // Guards against a second start while the first promise is still settling.
  const running = useRef(false);

  const start = useCallback(async (request: GenerateRequest): Promise<GenerationSummary | null> => {
    if (running.current) return null;
    running.current = true;
    dispatch({ type: 'start', total: request.profile.stop_at, now: Date.now() });
    try {
      const summary = await api.generate(request, (event) => dispatch({ type: 'event', event }));
      dispatch({ type: 'finished', summary });
      return summary;
    } catch (e) {
      dispatch({ type: 'failed', error: errorMessage(e) });
      return null;
    } finally {
      running.current = false;
    }
  }, []);

  const stop = useCallback(() => void api.stopGeneration().catch(() => undefined), []);

  const setPaused = useCallback(async (paused: boolean) => {
    try {
      await api.setGenerationPaused(paused);
      dispatch({ type: 'paused', paused });
    } catch {
      /* the job just ended; the finished/failed action will follow */
    }
  }, []);

  const reset = useCallback(() => dispatch({ type: 'reset' }), []);

  return { state, start, stop, setPaused, reset };
}
