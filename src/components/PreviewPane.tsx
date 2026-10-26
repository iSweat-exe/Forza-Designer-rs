import { memo } from 'react';

interface Props {
  src: string | null;
  alt: string;
  emptyText: string;
}

/** Image on a checkerboard so transparent regions read as transparent. */
export const PreviewPane = memo(function PreviewPane({ src, alt, emptyText }: Props) {
  return (
    <div className="preview checkerboard">
      {src ? <img src={src} alt={alt} draggable={false} /> : <p className="muted">{emptyText}</p>}
    </div>
  );
});
