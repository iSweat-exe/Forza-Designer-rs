import { useCallback, useEffect, useMemo, useState } from 'react';

import { api } from '../api/commands';
import type { DocumentInfo, ImageInfo, Profile } from '../api/types';
import { useAsyncResource } from '../hooks/useAsyncResource';
import { useElapsedSeconds } from '../hooks/useElapsed';
import { useGeneration } from '../hooks/useGeneration';
import { useI18n } from '../i18n/I18nProvider';
import { baseName, estimateRemaining, formatDuration } from '../lib';
import { ImagePicker } from './ImagePicker';
import { PreviewPane } from './PreviewPane';
import { ProfileForm } from './ProfileForm';
import { ProgressBar } from './ProgressBar';

const DEFAULT_PRESET = 'balanced';

interface Props {
  /** Called with the saved document once a run finishes. */
  onDocument: (doc: DocumentInfo) => void;
  onGoInject: () => void;
}

export function GenerateTab({ onDocument, onGoInject }: Props) {
  const { t } = useI18n();
  const presets = useAsyncResource(api.listProfiles);
  const { state, start, stop, setPaused, reset } = useGeneration();

  const [imagePath, setImagePath] = useState<string | null>(null);
  const [imageInfo, setImageInfo] = useState<ImageInfo | null>(null);
  const [imageError, setImageError] = useState<string | null>(null);
  const [sticker, setSticker] = useState(false);
  const [presetName, setPresetName] = useState(DEFAULT_PRESET);
  const [draft, setDraft] = useState<Profile | null>(null);

  const busy = state.phase === 'running' || state.phase === 'paused';

  // Seed the editable draft from the chosen preset.
  const presetList = presets.data;
  useEffect(() => {
    if (!presetList) return;
    const chosen = presetList.find((p) => p.name === presetName) ?? presetList[0];
    if (chosen) setDraft(chosen);
  }, [presetList, presetName]);

  // Load thumbnail + size for the picked image; ignore results of superseded picks.
  useEffect(() => {
    if (!imagePath) return;
    let cancelled = false;
    setImageError(null);
    api
      .describeImage(imagePath)
      .then((info) => !cancelled && setImageInfo(info))
      .catch((e: unknown) => {
        if (!cancelled) {
          setImageInfo(null);
          setImageError(String(e));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [imagePath]);

  const onPath = useCallback(
    (p: string) => {
      setImagePath(p);
      reset();
    },
    [reset],
  );

  const run = async () => {
    if (!imagePath || !draft) return;
    const summary = await start({ imagePath, profile: draft, stickerMode: sticker });
    if (summary) {
      try {
        onDocument(await api.loadDocument(summary.path));
      } catch {
        /* the summary card still shows where the file was saved */
      }
    }
  };

  const elapsed = useElapsedSeconds(state.startedAt, state.phase === 'running');
  const remaining = useMemo(
    () => (state.phase === 'running' ? estimateRemaining(state.shapes, state.total, elapsed) : null),
    [state.phase, state.shapes, state.total, elapsed],
  );

  const canStart = Boolean(imagePath && imageInfo && draft) && !busy;

  return (
    <div className="columns" role="tabpanel" id="panel-generate" aria-labelledby="tab-generate">
      <section className="card stack" aria-label={t('imageTitle')}>
        <h2>{t('imageTitle')}</h2>
        <ImagePicker path={imagePath} info={imageInfo} disabled={busy} onPath={onPath} />
        {imageError && <p className="error small">{imageError}</p>}
        <label className="check">
          <input type="checkbox" checked={sticker} disabled={busy} onChange={(e) => setSticker(e.target.checked)} />
          <span>
            {t('stickerMode')}
            <span className="muted small block">{t('stickerHint')}</span>
          </span>
        </label>

        {presets.data && draft ? (
          <ProfileForm
            presets={presets.data}
            presetName={presetName}
            draft={draft}
            disabled={busy}
            onPreset={setPresetName}
            onChange={setDraft}
          />
        ) : (
          presets.error && <p className="error small">{presets.error}</p>
        )}

        <div className="actions">
          {!busy ? (
            <button className="btn primary" onClick={run} disabled={!canStart}>
              {t('start')}
            </button>
          ) : (
            <>
              <button className="btn" onClick={() => setPaused(state.phase === 'running')}>
                {state.phase === 'running' ? t('pause') : t('resume')}
              </button>
              <button className="btn danger" onClick={stop}>
                {t('stop')}
              </button>
            </>
          )}
        </div>
      </section>

      <section className="card stack" aria-label={t('previewTitle')}>
        <h2>{t('previewTitle')}</h2>
        <PreviewPane src={state.preview} alt={t('previewTitle')} emptyText={t('previewEmpty')} />

        {(busy || state.phase === 'done') && (
          <div className="stack tight" aria-live="polite">
            <ProgressBar value={state.shapes} max={state.total} label={t('shapesProgress', { n: state.shapes, total: state.total })} />
            <div className="row spread small">
              <span>
                {state.phase === 'paused' ? t('paused') : t('shapesProgress', { n: state.shapes, total: state.total })}
              </span>
              <span className="muted">
                {state.rms !== null && t('rmsLabel', { rms: state.rms.toFixed(2) })}
                {state.phase === 'running' && ` · ${t('elapsed', { t: formatDuration(elapsed) })}`}
                {remaining !== null && ` · ${t('remaining', { t: formatDuration(remaining) })}`}
              </span>
            </div>
          </div>
        )}

        {state.warnings.map((w, i) => (
          <p key={i} className="warn small">
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
              <button className="btn primary" onClick={onGoInject}>
                {t('useForInjection')}
              </button>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}
