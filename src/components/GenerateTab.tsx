import { useCallback, useState } from 'react';

import { api } from '../api/commands';
import type { DocumentInfo } from '../api/types';
import { useAsyncResource } from '../hooks/useAsyncResource';
import { useElapsedSeconds } from '../hooks/useElapsed';
import { useGeneration } from '../hooks/useGeneration';
import { useImageSelection } from '../hooks/useImageSelection';
import { useProfileDraft } from '../hooks/useProfileDraft';
import { useI18n } from '../i18n/I18nProvider';
import { GenerationStatus } from './GenerationStatus';
import { ImagePicker } from './ImagePicker';
import { PreviewPane } from './PreviewPane';
import { ProfileForm } from './ProfileForm';

interface Props {
  /** Called with the saved document once a run finishes. */
  onDocument: (doc: DocumentInfo) => void;
  onGoInject: () => void;
}

export function GenerateTab({ onDocument, onGoInject }: Props) {
  const { t } = useI18n();
  const presets = useAsyncResource(api.listProfiles);
  const { presetName, draft, select, change } = useProfileDraft(presets.data);
  const image = useImageSelection();
  const { state, start, stop, setPaused, reset } = useGeneration();
  const [sticker, setSticker] = useState(false);

  const busy = state.phase === 'running' || state.phase === 'paused';
  const elapsed = useElapsedSeconds(state.startedAt, state.phase === 'running');

  const { select: selectImage } = image;
  const onPath = useCallback(
    (p: string) => {
      selectImage(p);
      reset();
    },
    [selectImage, reset],
  );

  const run = async () => {
    if (!image.path || !draft) return;
    const summary = await start({ imagePath: image.path, profile: draft, stickerMode: sticker });
    if (!summary) return;
    try {
      onDocument(await api.loadDocument(summary.path));
    } catch {
      /* the result card still shows where the file was saved */
    }
  };

  const canStart = Boolean(image.path && image.info && draft) && !busy;

  return (
    <div className="columns" role="tabpanel" id="panel-generate" aria-labelledby="tab-generate">
      <section className="card stack" aria-label={t('imageTitle')}>
        <h2>{t('imageTitle')}</h2>
        <ImagePicker path={image.path} info={image.info} disabled={busy} onPath={onPath} />
        {image.error && <p className="error small">{image.error}</p>}
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
            onPreset={select}
            onChange={change}
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
        <GenerationStatus state={state} elapsedSeconds={elapsed} onUseForInjection={onGoInject} />
      </section>
    </div>
  );
}
