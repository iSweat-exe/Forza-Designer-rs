import { render, type RenderOptions } from '@testing-library/react';
import type { ReactElement } from 'react';

import type { Profile } from './api/types';
import { I18nProvider } from './i18n/I18nProvider';

/** Render inside the i18n provider (English). */
export function renderWithI18n(ui: ReactElement, options?: RenderOptions) {
  return render(<I18nProvider initial="en">{ui}</I18nProvider>, options);
}

export function makeProfile(over: Partial<Profile> = {}): Profile {
  return {
    name: 'balanced',
    description: 'Balanced',
    max_preview_size: 500,
    max_resolution: 1200,
    max_threads: 0,
    mutated_samples: 200,
    preview_every: 1,
    random_samples: 1000,
    save_at: [],
    save_every: 100,
    stop_at: 3000,
    shape_types: ['rotated_ellipse'],
    shape_alpha: 255,
    ...over,
  };
}

/** A promise whose settlement the test controls. */
export function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}
