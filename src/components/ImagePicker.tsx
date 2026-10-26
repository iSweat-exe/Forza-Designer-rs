import { useCallback } from 'react';

import { pickImage } from '../api/commands';
import type { ImageInfo } from '../api/types';
import { useFileDrop } from '../hooks/useFileDrop';
import { useI18n } from '../i18n/I18nProvider';
import { baseName, isImagePathLike } from '../lib';

interface Props {
  path: string | null;
  info: ImageInfo | null;
  disabled: boolean;
  onPath: (path: string) => void;
}

export function ImagePicker({ path, info, disabled, onPath }: Props) {
  const { t } = useI18n();

  const handleDrop = useCallback(
    (dropped: string) => {
      if (isImagePathLike(dropped)) onPath(dropped);
    },
    [onPath],
  );
  const hovering = useFileDrop(!disabled, handleDrop);

  const browse = async () => {
    const picked = await pickImage(t('pickImage'));
    if (picked) onPath(picked);
  };

  return (
    <button
      type="button"
      className={hovering ? 'dropzone hovering' : 'dropzone'}
      onClick={browse}
      disabled={disabled}
      aria-label={t('pickImage')}
    >
      {info ? (
        <>
          <img src={info.thumbnail} alt="" className="thumb checkerboard" />
          <span className="dropzone-text">
            <strong>{path ? baseName(path) : ''}</strong>
            <span className="muted small">
              {t('imageInfo', { w: info.width, h: info.height, alpha: info.has_alpha ? t('hasAlpha') : '' })}
            </span>
          </span>
        </>
      ) : (
        <span className="dropzone-text">{t('dropHint')}</span>
      )}
    </button>
  );
}
