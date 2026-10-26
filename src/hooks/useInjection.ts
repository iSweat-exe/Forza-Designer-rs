import { useCallback, useReducer, useRef } from 'react';

import { api, errorMessage } from '../api/commands';
import type { InjectEvent, InjectReport } from '../api/types';

export type InjectionPhase = 'idle' | 'running' | 'done' | 'error';

export interface InjectionState {
  phase: InjectionPhase;
  pid: number | null;
  build: string | null;
  /** Template size currently being searched for. */
  scanningLayers: number | null;
  scanned: number;
  scanTotal: number;
  rttiFallback: boolean;
  rttiPhase: number | null;
  slots: number | null;
  written: number;
  writeTotal: number;
  report: InjectReport | null;
  error: string | null;
}

export const initialInjectionState: InjectionState = {
  phase: 'idle',
  pid: null,
  build: null,
  scanningLayers: null,
  scanned: 0,
  scanTotal: 0,
  rttiFallback: false,
  rttiPhase: null,
  slots: null,
  written: 0,
  writeTotal: 0,
  report: null,
  error: null,
};

export type InjectionAction =
  | { type: 'start' }
  | { type: 'event'; event: InjectEvent }
  | { type: 'finished'; report: InjectReport }
  | { type: 'failed'; error: string }
  | { type: 'reset' };

export function injectionReducer(state: InjectionState, action: InjectionAction): InjectionState {
  switch (action.type) {
    case 'start':
      return { ...initialInjectionState, phase: 'running' };
    case 'event': {
      const e = action.event;
      switch (e.kind) {
        case 'attached':
          return { ...state, pid: e.pid, build: e.build };
        case 'scanning':
          return { ...state, scanningLayers: e.layers, scanned: 0, scanTotal: 0 };
        case 'scan_progress':
          return { ...state, scanned: e.scanned, scanTotal: e.total };
        case 'rtti_fallback':
          return { ...state, rttiFallback: true, scanned: 0, scanTotal: 0 };
        case 'rtti_phase':
          return { ...state, rttiPhase: e.phase, scanned: 0, scanTotal: 0 };
        case 'located':
          return { ...state, slots: e.slots, scanningLayers: null, rttiPhase: null };
        case 'writing':
          return { ...state, written: e.written, writeTotal: e.total };
      }
      return state;
    }
    case 'finished':
      return { ...state, phase: 'done', report: action.report, written: action.report.written, writeTotal: action.report.total };
    case 'failed':
      return { ...state, phase: 'error', error: action.error };
    case 'reset':
      return initialInjectionState;
  }
}

export function useInjection() {
  const [state, dispatch] = useReducer(injectionReducer, initialInjectionState);
  const running = useRef(false);

  const start = useCallback(async (path: string, game: string) => {
    if (running.current) return;
    running.current = true;
    dispatch({ type: 'start' });
    try {
      const report = await api.inject(path, game, (event) => dispatch({ type: 'event', event }));
      dispatch({ type: 'finished', report });
    } catch (e) {
      dispatch({ type: 'failed', error: errorMessage(e) });
    } finally {
      running.current = false;
    }
  }, []);

  const cancel = useCallback(() => void api.cancelInjection().catch(() => undefined), []);
  const reset = useCallback(() => dispatch({ type: 'reset' }), []);

  return { state, start, cancel, reset };
}
