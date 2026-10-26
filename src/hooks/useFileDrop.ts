import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useEffect, useState } from 'react';

/** Calls `onDrop` with the first dropped path; returns whether a drag is hovering. */
export function useFileDrop(enabled: boolean, onDrop: (path: string) => void): boolean {
  const [hovering, setHovering] = useState(false);

  useEffect(() => {
    if (!enabled) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    let webview: ReturnType<typeof getCurrentWebview>;
    try {
      webview = getCurrentWebview();
    } catch {
      return; // not running inside Tauri (tests, plain browser)
    }
    webview
      .onDragDropEvent((event) => {
        const p = event.payload;
        if (p.type === 'enter' || p.type === 'over') setHovering(true);
        else if (p.type === 'leave') setHovering(false);
        else if (p.type === 'drop') {
          setHovering(false);
          const first = p.paths[0];
          if (first) onDrop(first);
        }
      })
      .then((fn) => {
        // The component may have unmounted before the listener was registered.
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => undefined); // not running inside Tauri (tests, plain browser)
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [enabled, onDrop]);

  return hovering;
}
