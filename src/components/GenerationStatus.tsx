import type { GenerationState } from '../hooks/useGeneration';
import { useI18n } from '../i18n/I18nProvider';
import { baseName, estimateRemaining, formatDuration } from '../lib';
import { ProgressBar } from './ProgressBar';

interface Props {
  state: GenerationState;
  elapsedSeconds: number;
  onUseForInjection: () => void;
}

/** Progress, warnings, errors and the final result card of a generation run. */
export function GenerationStatus({ state, elapsedSeconds, onUseForInjection }: Props) {
  const { t } = useI18n();
  const active = state.phase === 'running' || state.phase === 'paused';
  const remaining = state.phase === 'running' ? estimateRemaining(state.shapes, state.total, elapsedSeconds) : null;
  const count = t('shapesProgress', { n: state.shapes, total: state.total });

  return (
    <>
      {(active || state.phase === 'done') && (
        <div className="stack tight" aria-live="polite">
          <ProgressBar value={state.shapes} max={state.total} label={count} />
          <div className="row spread small">
            <span>{state.phase === 'paused' ? t('paused') : count}</span>
            <span className="muted">
              {state.rms !== null && t('rmsLabel', { rms: state.rms.toFixed(2) })}
              {state.phase === 'running' && ` · ${t('elapsed', { t: formatDuration(elapsedSeconds) })}`}
              {remaining !== null && ` · ${t('remaining', { t: formatDuration(remaining) })}`}
            </span>
          </div>
        </div>
      )}

      {state.warnings.map((w, i) => (
        <p key={`${i}-${w}`} className="warn small">
          {w}
        </p>
      ))}

      {state.phase === 'error' && (
        <p className="error" role="alert">
          <strong>{t('error')}:</strong> {state.error}
        </p>
      )}

      {state.phase === 'done' && state.summary && (
        <div className="result stack tight">
          <strong>{t('doneTitle')}</strong>
          <span>
            {t(state.summary.stopped_early ? 'doneEarly' : 'doneBody', {
              n: state.summary.shapes,
              rms: state.summary.rms.toFixed(2),
            })}
          </span>
          {state.summary.message && <span className="warn small">{state.summary.message}</span>}
          <span className="muted small">
            {t('savedTo')} <code title={state.summary.path}>{baseName(state.summary.path)}</code>
            {state.checkpoints > 0 && ` · ${t('checkpoints', { n: state.checkpoints })}`}
          </span>
          <div className="actions">
            <button className="btn primary" onClick={onUseForInjection}>
              {t('useForInjection')}
            </button>
          </div>
        </div>
      )}
    </>
  );
}
