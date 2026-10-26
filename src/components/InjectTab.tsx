import { useEffect, useState } from 'react';

import { api, errorMessage, pickDocument } from '../api/commands';
import type { DocumentInfo, GameInfo, InjectReport } from '../api/types';
import { useAsyncResource } from '../hooks/useAsyncResource';
import { useInjection, type InjectionState } from '../hooks/useInjection';
import { useI18n } from '../i18n/I18nProvider';
import type { StringKey } from '../i18n/strings';
import { baseName } from '../lib';
import { PreviewPane } from './PreviewPane';
import { ProgressBar } from './ProgressBar';

interface Props {
  document: DocumentInfo | null;
  onDocument: (doc: DocumentInfo | null) => void;
}

const DOC_PREVIEW_SIDE = 560;

export function InjectTab({ document, onDocument }: Props) {
  const { t } = useI18n();
  const games = useAsyncResource(api.listGames);
  const { state, start, cancel, reset } = useInjection();
  const [gameKey, setGameKey] = useState<string | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const gameList = games.data;
  const game = gameList?.find((g) => g.key === gameKey) ?? gameList?.[0] ?? null;
  const busy = state.phase === 'running';

  // Render the chosen document the way the game will draw it.
  const docPath = document?.path ?? null;
  useEffect(() => {
    if (!docPath) {
      setPreview(null);
      return;
    }
    let cancelled = false;
    api
      .renderDocument(docPath, DOC_PREVIEW_SIDE)
      .then((png) => !cancelled && setPreview(png))
      .catch(() => !cancelled && setPreview(null));
    return () => {
      cancelled = true;
    };
  }, [docPath]);

  const choose = async () => {
    const picked = await pickDocument(t('pickDocument'));
    if (!picked) return;
    try {
      setLoadError(null);
      reset();
      onDocument(await api.loadDocument(picked));
    } catch (e) {
      setLoadError(`${t('loadError')}: ${errorMessage(e)}`);
    }
  };

  const canInject = Boolean(document && game?.injection_supported) && !busy;

  return (
    <div className="columns" role="tabpanel" id="panel-inject" aria-labelledby="tab-inject">
      <section className="card stack" aria-label={t('injectTitle')}>
        <h2>{t('injectTitle')}</h2>

        <div className="field">
          <span className="label">{t('game')}</span>
          <div className="row">
            <select
              value={game?.key ?? ''}
              disabled={busy || !gameList}
              onChange={(e) => setGameKey(e.target.value)}
              aria-label={t('game')}
            >
              {gameList?.map((g) => (
                <option key={g.key} value={g.key}>
                  {g.label}
                </option>
              ))}
            </select>
            {game && <GameBadge game={game} />}
            <button className="btn small-btn" onClick={games.reload} disabled={busy}>
              {t('refresh')}
            </button>
          </div>
        </div>

        <div className="field">
          <span className="label">{t('document')}</span>
          <div className="row">
            <button className="btn" onClick={choose} disabled={busy}>
              {t('pickDocument')}
            </button>
            {document && <code title={document.path}>{baseName(document.path)}</code>}
          </div>
          {document ? (
            <span className="muted small">
              {t('docInfo', { n: document.shape_count, w: document.image_size[0], h: document.image_size[1] })}
            </span>
          ) : (
            <span className="muted small">{t('noDocument')}</span>
          )}
          {loadError && <span className="error small">{loadError}</span>}
        </div>

        {game && !game.injection_supported && <p className="warn small">{t('windowsOnly')}</p>}

        <div className="checklist">
          <h3>{t('checklistTitle')}</h3>
          <ol>
            <li>{t('checklist1')}</li>
            <li>{t('checklist2', { n: document?.shape_count ?? 'N' })}</li>
            <li>{t('checklist3')}</li>
            <li>{t('checklist4')}</li>
          </ol>
        </div>

        <div className="actions">
          {!busy ? (
            <button className="btn primary" disabled={!canInject} onClick={() => document && game && start(document.path, game.key)}>
              {t('inject', { game: game?.label ?? '' })}
            </button>
          ) : (
            <button className="btn danger" onClick={cancel}>
              {t('cancel')}
            </button>
          )}
        </div>
      </section>

      <section className="card stack" aria-label={t('previewTitle')}>
        <h2>{t('previewTitle')}</h2>
        <PreviewPane src={preview} alt={t('previewTitle')} emptyText={t('noDocument')} />
        <InjectStatus state={state} />
      </section>
    </div>
  );
}

function GameBadge({ game }: { game: GameInfo }) {
  const { t } = useI18n();
  return (
    <span className={game.running ? 'badge ok' : 'badge'}>{game.running ? t('gameRunning') : t('gameNotRunning')}</span>
  );
}

function InjectStatus({ state }: { state: InjectionState }) {
  const { t } = useI18n();
  if (state.phase === 'idle') return null;

  const scanning = state.phase === 'running' && state.slots === null;
  const writing = state.slots !== null;

  return (
    <div className="stack tight" aria-live="polite">
      {state.pid !== null && (
        <p className="small">
          {t('attached', { pid: state.pid, build: state.build ? t('buildSuffix', { build: state.build }) : '' })}
        </p>
      )}

      {scanning && (
        <>
          {state.rttiFallback && <p className="warn small">{t('rttiFallback')}</p>}
          <p className="small">
            {state.rttiPhase !== null
              ? t(`rttiPhase${state.rttiPhase}` as StringKey)
              : state.scanningLayers !== null
                ? t('scanning', { n: state.scanningLayers })
                : null}
          </p>
          <ProgressBar value={state.scanned} max={state.scanTotal} indeterminate={state.scanTotal === 0} />
          {state.scanTotal > 0 && <p className="muted small">{t('scanRegions', { done: state.scanned, total: state.scanTotal })}</p>}
        </>
      )}

      {writing && state.slots !== null && (
        <>
          <p className="small">{t('located', { n: state.slots })}</p>
          <ProgressBar value={state.written} max={state.writeTotal} />
          {state.phase === 'running' && <p className="muted small">{t('writing', { written: state.written, total: state.writeTotal })}</p>}
        </>
      )}

      {state.phase === 'error' && (
        <p className="error" role="alert">
          <strong>{t('injectFailed')}:</strong> {state.error}
        </p>
      )}
      {state.phase === 'done' && state.report && <InjectSummary report={state.report} />}
    </div>
  );
}

function InjectSummary({ report }: { report: InjectReport }) {
  const { t } = useI18n();
  const mismatch = report.game_build && report.validated_build && report.game_build !== report.validated_build;
  const ok = report.written > 0 && report.skipped === 0;
  return (
    <div className={ok ? 'result stack tight' : 'result warnbox stack tight'}>
      <strong>{t('injectDone', { written: report.written, total: report.total })}</strong>
      {report.skipped > 0 && <span className="small">{t('injectSkipped', { n: report.skipped })}</span>}
      <span className="muted small">
        {t('injectMix')}:{' '}
        {Object.entries(report.type_counts)
          .map(([k, v]) => `${t(`shape_${k}` as StringKey)} ${v}`)
          .join(', ')}
      </span>
      {mismatch && <span className="warn small">{t('buildMismatch', { actual: report.game_build ?? '', expected: report.validated_build ?? '' })}</span>}
      {report.overrides_applied.length > 0 && (
        <span className="muted small">{t('overridesApplied', { keys: report.overrides_applied.join(', ') })}</span>
      )}
    </div>
  );
}
